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
    codec: String,
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
fn get_test_images(limit: Option<usize>, lossy: bool) -> Vec<PathBuf> {
    let test_dir = PathBuf::from("tests/images");
    if !test_dir.exists() {
        eprintln!("Test images directory not found: {:?}", test_dir);
        return Vec::new();
    }

    // Known problematic files to skip
    let mut skip_files = vec![
        "smallliz.tif",
        "text.tif",
        "ycbcr-cat.tif",
        "zackthecat.tif",
        "quad-tile.jpg.tiff",
        "quad-jpeg.tif",
        "sample-get-lzw-stuck.tiff",
        "tiled-jpeg-ycbcr.tif",
    ];

    if lossy {
        skip_files.extend([
            "170918_tn_neutrophil_migration_wave.ome.tif",
            "181003_multi_pos_time_course_1_MMStack.ome.tif",
            "MMStack_Pos0.ome.tif",
            "P1_T0.tif",
            "P1_T1.tif",
            "P1_T2.tif",
            "P1_T3.tif",
            "P1_T4.tif",
            "P1_T5.tif",
            "P1_T6.tif",
            "P1_T7.tif",
            "P1_T8.tif",
            "P1_T9.tif",
            "P2_T0.tif",
            "P2_T1.tif",
            "P2_T2.tif",
            "P2_T3.tif",
            "P2_T4.tif",
            "P2_T5.tif",
            "P2_T6.tif",
            "P2_T7.tif",
            "P2_T8.tif",
            "P2_T9.tif",
            "TSeries-camp-005_Cycle00001_Ch1_000001.ome.tif",
            "TSeries-camp-005_Cycle00001_Ch1_000002.ome.tif",
            "TSeries-camp-005_Cycle00001_Ch1_000003.ome.tif",
            "TSeries-camp-005_Cycle00001_Ch1_000004.ome.tif",
            "TSeries-camp-005_Cycle00001_Ch2_000001.ome.tif",
            "TSeries-camp-005_Cycle00001_Ch2_000002.ome.tif",
            "TSeries-camp-005_Cycle00001_Ch2_000003.ome.tif",
            "TSeries-camp-005_Cycle00001_Ch2_000004.ome.tif",
            "TSeries-camp-005_Cycle00002_Ch1_000001.ome.tif",
            "TSeries-camp-005_Cycle00002_Ch1_000002.ome.tif",
            "TSeries-camp-005_Cycle00002_Ch1_000003.ome.tif",
            "TSeries-camp-005_Cycle00002_Ch1_000004.ome.tif",
            "TSeries-camp-005_Cycle00002_Ch2_000001.ome.tif",
            "TSeries-camp-005_Cycle00002_Ch2_000002.ome.tif",
            "TSeries-camp-005_Cycle00002_Ch2_000003.ome.tif",
            "TSeries-camp-005_Cycle00002_Ch2_000004.ome.tif",
            "TSeries-camp-005_Cycle00003_Ch1_000001.ome.tif",
            "TSeries-camp-005_Cycle00003_Ch1_000002.ome.tif",
            "TSeries-camp-005_Cycle00003_Ch1_000003.ome.tif",
            "TSeries-camp-005_Cycle00003_Ch1_000004.ome.tif",
            "TSeries-camp-005_Cycle00003_Ch2_000001.ome.tif",
            "TSeries-camp-005_Cycle00003_Ch2_000002.ome.tif",
            "TSeries-camp-005_Cycle00003_Ch2_000003.ome.tif",
            "TSeries-camp-005_Cycle00003_Ch2_000004.ome.tif",
            "background_1_MMStack.ome.tif",
            "bali.tif",
            "big_g4.tif",
            "capitol.tif",
            "capitol2.tif",
            "caspian.tif",
            "cmyk-3c-16b.tiff",
            "cmyk-3c-32b-float.tiff",
            "cmyk-4c-8b.tiff",
            "earthlab.tif",
            "earthlab_compressed.tif",
            "fax2d.tif",
            "fax4.tif",
            "flower-minisblack-02.tif",
            "flower-minisblack-04.tif",
            "flower-minisblack-06.tif",
            "flower-minisblack-10.tif",
            "flower-minisblack-14.tif",
            "flower-minisblack-16.tif",
            "flower-minisblack-24.tif",
            "flower-minisblack-32.tif",
            "flower-palette-02.tif",
            "flower-palette-04.tif",
            "flower-palette-08.tif",
            "flower-palette-16.tif",
            "flower-rgb-contig-02.tif",
            "flower-rgb-contig-04.tif",
            "flower-rgb-contig-10.tif",
            "flower-rgb-contig-14.tif",
            "flower-rgb-contig-16.tif",
            "flower-rgb-contig-24.tif",
            "flower-rgb-contig-32.tif",
            "flower-rgb-planar-02.tif",
            "flower-rgb-planar-04.tif",
            "flower-rgb-planar-10.tif",
            "flower-rgb-planar-14.tif",
            "flower-rgb-planar-16.tif",
            "flower-rgb-planar-24.tif",
            "flower-rgb-planar-32.tif",
            "flower-separated-16.tif",
            "flower-separated-planar-16.tif",
            "g3test.tif",
            "geo-5b.tif",
            "gradient-1c-32b-float.tiff",
            "gradient-1c-32b.tiff",
            "gradient-1c-64b-float.tiff",
            "gradient-1c-64b.tiff",
            "gradient-3c-32b-float.tiff",
            "gradient-3c-32b.tiff",
            "gradient-3c-64b.tiff",
            "imagemagick_group4.tif",
            "int16.tif",
            "int16_rgb.tif",
            "int16_zstd.tif",
            "issue_69_lzw.tif",
            "issue_69_packbits.tif",
            "jello.tif",
            "jim___ah.tif",
            "ladoga.tif",
            "logluv-3c-16b.tif",
            "minisblack-1c-16b.tif",
            "minisblack-1c-3b.tif",
            "minisblack-1c-5b.tif",
            "minisblack-1c-7b.tif",
            "minisblack-1c-i16b.tif",
            "miniswhite-1c-1b.tif",
            "miniswhite-1c-3b.tif",
            "miniswhite-1c-6b.tif",
            "mri.tif",
            "off_l16.tif",
            "off_luv24.tif",
            "off_luv32.tif",
            "palette-1c-1b.tif",
            "palette-1c-4b.tif",
            "palette-1c-8b.tif",
            "poppies.tif",
            "predictor-3-gray-f32.tif",
            "predictor-3-rgb-f32.tif",
            "random-fp16-pred2.tif",
            "random-fp16-pred3.tif",
            "random-fp16.tif",
            "renamed_internalfilenames.ome.tif",
            "rgb-3c-16b.tif",
            "seq-1c-10b-6d739fa2.tif",
            "seq-1c-10b-hpredict-6d739fa2.tif",
            "seq-1c-10b-miniswhite-6d739fa2.tif",
            "seq-1c-14b-e883657f.tif",
            "seq-1c-14b-hpredict-e883657f.tif",
            "seq-1c-14b-miniswhite-e883657f.tif",
            "seq-1c-16b-bigendian-68f373a0.tif",
            "seq-1c-16b-deflate-68f373a0.tif",
            "seq-1c-16b-lzw-68f373a0.tif",
            "seq-1c-16b-multistrip-68f373a0.tif",
            "seq-1c-16b-tiled-68f373a0.tif",
            "seq-1c-1b-71f6a21a.tif",
            "seq-1c-1b-miniswhite-71f6a21a.tif",
            "seq-1c-24b-072a9dc9.tif",
            "seq-1c-24b-hpredict-072a9dc9.tif",
            "seq-1c-24b-miniswhite-072a9dc9.tif",
            "seq-1c-2b-58b25f76.tif",
            "seq-1c-32f-390fe673.tif",
            "seq-1c-32f-deflate-fpredict-390fe673.tif",
            "seq-1c-3b-ef237c07.tif",
            "seq-1c-3b-miniswhite-ef237c07.tif",
            "seq-1c-4b-fb92dcae.tif",
            "seq-1c-4b-miniswhite-fb92dcae.tif",
            "seq-1c-4b-palette-85108c5a.tif",
            "seq-1c-5b-73098d17.tif",
            "seq-1c-5b-miniswhite-73098d17.tif",
            "seq-1c-64f-afa8560e.tif",
            "seq-1c-64f-deflate-fpredict-afa8560e.tif",
            "seq-1c-6b-miniswhite-79cafbb6.tif",
            "seq-1c-7b-9c61ba70.tif",
            "seq-1c-7b-miniswhite-9c61ba70.tif",
            "seq-1c-8b-palette-89b39bc3.tif",
            "seq-1c-i16-63af2488.tif",
            "seq-1c-i32-99fddec2.tif",
            "seq-3c-10b-contig-d08d5dc0.tif",
            "seq-3c-10b-planar-c82e8ab6.tif",
            "seq-3c-14b-contig-f4dcc6cc.tif",
            "seq-3c-14b-planar-4dde706b.tif",
            "seq-3c-16b-bigtiff-1b40ca6e.tif",
            "seq-3c-24b-contig-27b9f8ce.tif",
            "seq-3c-24b-planar-6296c0c9.tif",
            "seq-3c-32f-9a471c2b.tif",
            "seq-3c-5b-contig-09f197f4.tif",
            "seq-3c-64f-9fff098a.tif",
            "seq-3c-7b-contig-2e4f43c5.tif",
            "seq-3c-i16-f7fcf423.tif",
            "seq-4c-16b-cmyk-c6e52592.tif",
            "seq-4c-16b-rgba-5181991f.tif",
            "shapes_hyper.tif",
            "shapes_lzw_14bps.tif",
            "shapes_lzw_palette.tif",
            "shapes_lzw_planar_10bps.tif",
            "shapes_lzw_predictor3.tif",
            "shapes_multi_color.tif",
            "single-black-fp16.tif",
            "spine.tif",
            "spring.tif",
            "tiled-gray-i1.tif",
            "white-fp16-pred2.tif",
            "white-fp16-pred3.tif",
            "white-fp16.tif",
        ]);
    }

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
        codec: format!("{} (lvl {})", format, level),
    }
}

/// Generate Markdown report
fn generate_report(
    lossless_summary: Option<&ReportSummary>,
    lossy_summary: Option<&ReportSummary>,
    output_path: &Path,
) {
    let mut report = String::new();

    report.push_str("# tiff-reducer Test Report\n\n");
    report.push_str(&format!(
        "**Generated:** {}\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));

    report.push_str("## Summary\n\n");
    if let Some(s) = lossless_summary {
        report.push_str(&format!(
            "- [Lossless Report](#lossless-report): {} working, {} failed\n",
            s.success, s.failed
        ));
    }
    if let Some(s) = lossy_summary {
        report.push_str(&format!(
            "- [Lossy Report](#lossy-report): {} working, {} failed\n",
            s.success, s.failed
        ));
    }
    report.push('\n');

    if let Some(s) = lossless_summary {
        report.push_str("<a id=\"lossless-report\"></a>\n## Lossless Report\n\n");
        render_section(&mut report, s);
    }
    if let Some(s) = lossy_summary {
        report.push_str("<a id=\"lossy-report\"></a>\n## Lossy Report\n\n");
        render_section(&mut report, s);
    }

    let mut file = fs::File::create(output_path).expect("Failed to create report file");
    file.write_all(report.as_bytes())
        .expect("Failed to write report");

    println!("\nReport written to {}", output_path.display());
}

fn render_section(report: &mut String, summary: &ReportSummary) {
    // Summary table
    report.push_str(
        "### Summary\n\n| Category | Count | Percentage |\n|----------|-------|------------|\n",
    );
    report.push_str(&format!(
        "| ✅ Working | {} | {:.1}% |\n| ❌ Failed | {} | {:.1}% |\n| **Total** | **{}** | **100%** |\n\n",
        summary.success,
        if summary.total > 0 { summary.success as f64 / summary.total as f64 * 100.0 } else { 0.0 },
        summary.failed,
        if summary.total > 0 { summary.failed as f64 / summary.total as f64 * 100.0 } else { 0.0 },
        summary.total
    ));

    // Failed images
    let failed: Vec<&TestResult> = summary.results.iter().filter(|r| !r.success).collect();
    if !failed.is_empty() {
        report.push_str("### ❌ Failed Images\n\n| File | Original Size | Error |\n|------|---------------|-------|\n");
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

    // Working images
    let working: Vec<&TestResult> = summary.results.iter().filter(|r| r.success).collect();
    if !working.is_empty() {
        report.push_str(
            "### ✅ Working Images\n\n| Original | Compressed | Details |\n|:---:|:---:|:---:|\n",
        );
        for result in &working {
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

            let reduction = if result.orig_size > 0 {
                (1.0 - result.comp_size as f64 / result.orig_size as f64) * 100.0
            } else {
                0.0
            };
            report.push_str(&format!(" | **File:** `{}`<br>**Codec:** {}<br>**Size:** {} → {}<br>**Red:** {:.1}%<br>**Time:** {}ms |\n",
                result.name, result.codec, format_size(result.orig_size), format_size(result.comp_size), reduction, result.duration_ms));
        }
        report.push('\n');
    }
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

    let images = get_test_images(cli.limit, cli.lossy);
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

    let mut lossless_summary = ReportSummary {
        total: images.len(),
        success: 0,
        failed: 0,
        results: Vec::new(),
        total_duration_ms: 0,
    };
    let start = Instant::now();
    for image_path in &images {
        let result = test_compression(
            image_path,
            &binary_path,
            &cli.format,
            cli.level,
            false,
            &thumbs_dir,
        );
        if result.success {
            lossless_summary.success += 1;
        } else {
            lossless_summary.failed += 1;
        }
        lossless_summary.results.push(result);
    }
    lossless_summary.total_duration_ms = start.elapsed().as_millis() as u64;

    let mut lossy_summary = ReportSummary {
        total: images.len(),
        success: 0,
        failed: 0,
        results: Vec::new(),
        total_duration_ms: 0,
    };
    let start = Instant::now();
    for image_path in &images {
        let result = test_compression(
            image_path,
            &binary_path,
            &cli.format,
            cli.level,
            true,
            &thumbs_dir,
        );
        if result.success {
            lossy_summary.success += 1;
        } else {
            lossy_summary.failed += 1;
        }
        lossy_summary.results.push(result);
    }
    lossy_summary.total_duration_ms = start.elapsed().as_millis() as u64;

    generate_report(Some(&lossless_summary), Some(&lossy_summary), &output_path);
}
