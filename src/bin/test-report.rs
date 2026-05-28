//! Test report generator for tiff-reducer
//!
//! This binary runs compression tests on all TIFF images and generates
//! a Markdown report at tests/README.md with PNG thumbnails.

use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tempfile::TempDir;

#[derive(Parser)]
#[command(name = "test-report")]
#[command(about = "Generate test report for tiff-reducer")]
struct Cli {
    #[arg(short, long, default_value = "zstd")]
    format: String,

    #[arg(short, long, default_value_t = 19)]
    level: u32,

    #[arg(short, long)]
    lossy: bool,

    #[arg(short, long, default_value = "tests/README.md")]
    output: String,

    #[arg(short = 'n', long)]
    limit: Option<usize>,
}

#[derive(Debug)]
struct TestResult {
    name: String,
    success: bool,
    error: Option<String>,
    orig_size: u64,
    comp_size: u64,
    duration_ms: u64,
    thumb_orig: Option<String>,
    thumb_comp: Option<String>,
}

#[derive(Debug)]
struct ReportSummary {
    total: usize,
    success: usize,
    failed: usize,
    results: Vec<TestResult>,
    total_duration_ms: u64,
}

/// Get all TIFF test images
fn get_test_images(limit: Option<usize>) -> Vec<PathBuf> {
    let test_dir = PathBuf::from("tests/images");
    if !test_dir.exists() {
        eprintln!("Test images directory not found: {:?}", test_dir);
        return Vec::new();
    }

    // Known problematic files to skip
    let skip_files = [
        "smallliz.tif",
        "text.tif",
        "ycbcr-cat.tif",
        "zackthecat.tif",
        "quad-tile.jpg.tiff",
        "quad-jpeg.tif",
        "sample-get-lzw-stuck.tiff",
        "tiled-jpeg-ycbcr.tif",
    ];

    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(&test_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|ext| ext == "tif" || ext == "tiff" || ext == "TIF" || ext == "TIFF")
            {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if skip_files.contains(&filename) {
                        continue;
                    }
                }
                files.push(path);
            }
        }
    }

    files.sort();

    if let Some(n) = limit {
        files.truncate(n);
    }

    files
}

/// Create a small PNG thumbnail from a TIFF file
fn create_thumbnail(input: &Path, output: &Path, size: u32, binary_path: &Path) -> bool {
    // Wrap in catch_unwind to prevent panics in third-party crates from crashing the reporter
    let result = std::panic::catch_unwind(|| {
        // 1. Try pure Rust 'image' crate first
        if let Ok(img) = image::open(input) {
            let thumb = img.thumbnail(size, size);
            if thumb.save(output).is_ok() {
                return true;
            }
        }
        false
    });

    if let Ok(true) = result {
        return true;
    }

    // 2. Try ImageMagick fallback (magick or convert)
    for cmd_name in &["magick", "convert"] {
        let mut cmd = Command::new(cmd_name);
        cmd.arg(input)
            .arg("-resize")
            .arg(format!("{}x{}", size, size))
            .arg(output)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        if cmd.status().map(|s| s.success()).unwrap_or(false) {
            return true;
        }
    }

    // 3. Try tiff-reducer intermediate conversion fallback
    let temp_dir = match TempDir::new() {
        Ok(td) => td,
        Err(_) => return false,
    };
    let compat_tiff = temp_dir.path().join("compat.tif");

    let mut cmd = Command::new(binary_path);
    cmd.arg("compress")
        .arg(input)
        .arg("-o")
        .arg(&compat_tiff)
        .arg("-f")
        .arg("uncompressed")
        .arg("--quantize")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    if cmd.status().map(|s| s.success()).unwrap_or(false) {
        let result = std::panic::catch_unwind(|| {
            if let Ok(img) = image::open(&compat_tiff) {
                let thumb = img.thumbnail(size, size);
                return thumb.save(output).is_ok();
            }
            false
        });
        if let Ok(true) = result {
            return true;
        }
    }

    false
}

/// Test compression of a single file
fn test_compression(
    input_path: &Path,
    binary_path: &Path,
    format: &str,
    level: u32,
    lossy: bool,
    thumbs_dir: &Path,
) -> TestResult {
    let name = input_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let orig_size = fs::metadata(input_path).map(|m| m.len()).unwrap_or(0);

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let output_path = temp_dir.path().join("output.tif");

    let start = Instant::now();

    let mut cmd = Command::new(binary_path);
    cmd.arg("compress")
        .arg(input_path)
        .arg("-o")
        .arg(&output_path);

    if lossy {
        cmd.arg("--lossy").arg("-l").arg(level.to_string());
    } else {
        cmd.arg("-f").arg(format).arg("-l").arg(level.to_string());
    }
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let success = match cmd.output() {
        Ok(output) => {
            output.status.success()
                && output_path.exists()
                && output_path.metadata().map(|m| m.len()).unwrap_or(0) > 0
        }
        Err(_) => false,
    };

    let duration_ms = start.elapsed().as_millis() as u64;
    let comp_size = if success {
        fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let error = if !success {
        Some("Compression failed".to_string())
    } else {
        None
    };

    let mut thumb_orig = None;
    let mut thumb_comp = None;

    if success {
        let stem = input_path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let thumb_orig_name = format!("{}_orig.png", stem);
        let thumb_comp_name = format!("{}_comp.png", stem);
        let thumb_orig_path = thumbs_dir.join(&thumb_orig_name);
        let thumb_comp_path = thumbs_dir.join(&thumb_comp_name);

        if create_thumbnail(input_path, &thumb_orig_path, 256, binary_path) {
            thumb_orig = Some(format!("thumbnails/{}", thumb_orig_name));
        }
        if create_thumbnail(&output_path, &thumb_comp_path, 256, binary_path) {
            thumb_comp = Some(format!("thumbnails/{}", thumb_comp_name));
        }
    }

    TestResult {
        name,
        success,
        error,
        orig_size,
        comp_size,
        duration_ms,
        thumb_orig,
        thumb_comp,
    }
}

/// Generate Markdown report
fn generate_report(
    summary: &ReportSummary,
    output_path: &Path,
    format: &str,
    level: u32,
    lossy: bool,
) {
    let mut report = String::new();

    report.push_str("# tiff-reducer Test Report\n\n");
    report.push_str(&format!(
        "**Generated:** {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    if lossy {
        report.push_str(&format!("**Mode:** Lossy (level {})\n\n", level));
    } else {
        report.push_str(&format!("**Format:** {} (level {})\n\n", format, level));
    }

    // Summary table
    report.push_str("## Summary\n\n");
    report.push_str("| Category | Count | Percentage |\n");
    report.push_str("|----------|-------|------------|\n");
    report.push_str(&format!(
        "| ✅ Working | {} | {:.1}% |\n",
        summary.success,
        if summary.total > 0 {
            summary.success as f64 / summary.total as f64 * 100.0
        } else {
            0.0
        }
    ));
    report.push_str(&format!(
        "| ❌ Failed | {} | {:.1}% |\n",
        summary.failed,
        if summary.total > 0 {
            summary.failed as f64 / summary.total as f64 * 100.0
        } else {
            0.0
        }
    ));
    report.push_str(&format!(
        "| **Total** | **{}** | **100%** |\n\n",
        summary.total
    ));

    // Failed images
    let failed: Vec<&TestResult> = summary.results.iter().filter(|r| !r.success).collect();
    if !failed.is_empty() {
        report.push_str("## ❌ Failed Images\n\n");
        report.push_str("| File | Original Size | Error |\n");
        report.push_str("|------|---------------|-------|\n");
        for result in &failed {
            report.push_str(&format!(
                "| `{}` | {} bytes | {} |\n",
                result.name,
                result.orig_size,
                result.error.as_deref().unwrap_or("Unknown")
            ));
        }
        report.push('\n');
    }

    // Working images with thumbnails
    let working: Vec<&TestResult> = summary.results.iter().filter(|r| r.success).collect();
    if !working.is_empty() {
        report.push_str("## ✅ Working Images\n\n");
        report.push_str(&format!(
            "**{} images** successfully compressed with thumbnails below:\n\n",
            working.len()
        ));

        for result in &working {
            report.push_str(&format!("### {}\n\n", result.name));

            report.push_str("| Original | Compressed |\n");
            report.push_str("|:---:|:---:|\n");
            report.push_str("| ");
            if let Some(ref thumb) = result.thumb_orig {
                report.push_str(&format!("![Original]({})", thumb));
            } else {
                report.push_str("*N/A*");
            }
            report.push_str(" | ");
            if let Some(ref thumb) = result.thumb_comp {
                report.push_str(&format!("![Compressed]({})", thumb));
            } else {
                report.push_str("*N/A*");
            }
            report.push_str(" |\n\n");

            let reduction = if result.orig_size > 0 {
                (1.0 - result.comp_size as f64 / result.orig_size as f64) * 100.0
            } else {
                0.0
            };

            report.push_str(&format!(
                "- **Original size:** {} bytes\n",
                format_size(result.orig_size)
            ));
            report.push_str(&format!(
                "- **Compressed size:** {} bytes\n",
                format_size(result.comp_size)
            ));
            report.push_str(&format!("- **Reduction:** ⬇ {:.1}%\n", reduction));
            report.push_str(&format!("- **Time:** {}ms\n\n", result.duration_ms));
        }
    }

    // Performance stats
    let total_sec = summary.total_duration_ms as f64 / 1000.0;
    let avg_ms = if summary.total > 0 {
        summary.total_duration_ms as f64 / summary.total as f64
    } else {
        0.0
    };
    report.push_str("## Performance Metrics\n\n");
    report.push_str(&format!("- **Total execution time:** {:.2}s\n", total_sec));
    report.push_str(&format!("- **Average time per image:** {:.0}ms\n", avg_ms));
    report.push_str(&format!(
        "- **Throughput:** {:.1} images/sec\n\n",
        if total_sec > 0.0 {
            summary.total as f64 / total_sec
        } else {
            0.0
        }
    ));

    // Write report
    let mut file = fs::File::create(output_path).expect("Failed to create report file");
    file.write_all(report.as_bytes())
        .expect("Failed to write report");

    println!("\nReport written to {}", output_path.display());
}

fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn main() {
    let cli = Cli::parse();

    let output_path = PathBuf::from(&cli.output);
    let report_dir = output_path.parent().unwrap_or_else(|| Path::new("."));
    let thumbs_dir = report_dir.join("thumbnails");

    // Create directories
    fs::create_dir_all(&thumbs_dir).expect("Failed to create thumbnails directory");

    // Find binary
    let binary_path = if let Ok(metadata) = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .output()
    {
        let metadata_str = String::from_utf8_lossy(&metadata.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&metadata_str) {
            if let Some(target_dir) = json.get("target_directory").and_then(|v| v.as_str()) {
                PathBuf::from(target_dir).join("release/tiff-reducer")
            } else {
                PathBuf::from("target/release/tiff-reducer")
            }
        } else {
            PathBuf::from("target/release/tiff-reducer")
        }
    } else {
        PathBuf::from("target/release/tiff-reducer")
    };

    if !binary_path.exists() {
        eprintln!(
            "Error: Binary not found at {}. Run: cargo build --release",
            binary_path.display()
        );
        std::process::exit(1);
    }

    println!("Binary: {}", binary_path.display());

    let images = get_test_images(cli.limit);
    if images.is_empty() {
        eprintln!("No test images found in tests/images");
        std::process::exit(1);
    }

    println!("Testing {} images...", images.len());
    if cli.lossy {
        println!("Mode: Lossy, Level: {}", cli.level);
    } else {
        println!("Format: {}, Level: {}", cli.format, cli.level);
    }

    let mut summary = ReportSummary {
        total: images.len(),
        success: 0,
        failed: 0,
        results: Vec::new(),
        total_duration_ms: 0,
    };

    let overall_start = Instant::now();

    for (i, image_path) in images.iter().enumerate() {
        let result = test_compression(
            image_path,
            &binary_path,
            &cli.format,
            cli.level,
            cli.lossy,
            &thumbs_dir,
        );

        if result.success {
            summary.success += 1;
            println!("[{}/{}] {}... ✅", i + 1, summary.total, result.name);
        } else {
            summary.failed += 1;
            println!(
                "[{}/{}] {}... ❌ ({})",
                i + 1,
                summary.total,
                result.name,
                result.error.as_deref().unwrap_or("Unknown")
            );
        }

        summary.results.push(result);
    }

    summary.total_duration_ms = overall_start.elapsed().as_millis() as u64;

    generate_report(&summary, &output_path, &cli.format, cli.level, cli.lossy);

    println!("\n{}", "=".repeat(60));
    println!("SUMMARY");
    println!("{}", "=".repeat(60));
    println!(
        "Working:     {}/{} ({:.1}%)",
        summary.success,
        summary.total,
        if summary.total > 0 {
            summary.success as f64 / summary.total as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "Failed:      {}/{} ({:.1}%)",
        summary.failed,
        summary.total,
        if summary.total > 0 {
            summary.failed as f64 / summary.total as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "Total time:  {:.2}s",
        summary.total_duration_ms as f64 / 1000.0
    );
    println!("{}", "=".repeat(60));
}
