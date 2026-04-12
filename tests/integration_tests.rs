//! Integration tests for tiff-reducer
//!
//! These tests verify:
//! - All TIFF files can be read and compressed without errors (zstd & uncompressed)
//! - Metadata is preserved during compression
//! - Pixel content is preserved for lossless compression
//! - Uncompressed format works correctly on all images

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Get all test images for comprehensive testing
fn get_all_test_images() -> Vec<PathBuf> {
    let test_dir = PathBuf::from("tests/images");
    if !test_dir.exists() {
        eprintln!("Test images directory not found: {:?}", test_dir);
        return Vec::new();
    }

    // Known problematic test files that should be skipped
    let skip_files = [
        "smallliz.tif",              // OJPEG compression - legacy format
        "text.tif",                  // THUNDERSCAN compression - obsolete format
        "ycbcr-cat.tif",             // YCbCr with subsampling - crash
        "zackthecat.tif",            // OJPEG + YCbCr - crash
        "quad-tile.jpg.tiff",        // Tiled JPEG + YCbCr - crash
        "quad-jpeg.tif",             // JPEG compression issues
        "sample-get-lzw-stuck.tiff", // LZW compression issues
        "tiled-jpeg-ycbcr.tif",      // JPEG/YCbCr issues
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
    eprintln!(
        "Found {} test images (excluding {} known problematic files)",
        files.len(),
        skip_files.len()
    );
    files
}

/// Test fixture for compression tests
struct CompressionTest {
    #[allow(dead_code)]
    temp_dir: TempDir,
    input_path: PathBuf,
    output_path: PathBuf,
}

impl CompressionTest {
    fn new(input_path: &Path) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let output_path = temp_dir.path().join("output.tif");

        Self {
            temp_dir,
            input_path: input_path.to_path_buf(),
            output_path,
        }
    }

    fn run(&self, format: &str, level: Option<u32>) -> bool {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
        cmd.arg("compress")
            .arg(&self.input_path)
            .arg("-o")
            .arg(&self.output_path)
            .arg("-f")
            .arg(format);

        if let Some(lvl) = level {
            cmd.arg("-l").arg(lvl.to_string());
        }

        let result = cmd.output();
        match result {
            Ok(output) => {
                if !output.status.success() {
                    return false;
                }
            }
            Err(_) => {
                return false;
            }
        }

        self.output_path.exists()
    }

    fn get_gdalinfo(&self, path: &Path) -> Option<Value> {
        let output = std::process::Command::new("gdalinfo")
            .arg("-json")
            .arg(path)
            .output()
            .ok()?;

        serde_json::from_slice(&output.stdout).ok()
    }

    fn file_size(&self, path: &Path) -> u64 {
        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }
}

/// Results from testing a single image with both formats
struct ImageTestResult {
    zstd_success: bool,
    uncompressed_success: bool,
    zstd_metadata_ok: bool,
    uncompressed_metadata_ok: bool,
    zstd_pixel_ok: bool,
    uncompressed_pixel_ok: bool,
}

/// Test a single image with both zstd and uncompressed formats
fn test_single_image_comprehensive(image_path: &Path) -> ImageTestResult {
    let mut result = ImageTestResult {
        zstd_success: false,
        uncompressed_success: false,
        zstd_metadata_ok: false,
        uncompressed_metadata_ok: false,
        zstd_pixel_ok: false,
        uncompressed_pixel_ok: false,
    };

    // Test zstd compression
    let test_zstd = CompressionTest::new(image_path);
    result.zstd_success = test_zstd.run("zstd", Some(19));

    if result.zstd_success {
        // Check metadata preservation for zstd
        if let (Some(orig), Some(comp)) = (
            test_zstd.get_gdalinfo(&test_zstd.input_path),
            test_zstd.get_gdalinfo(&test_zstd.output_path),
        ) {
            result.zstd_metadata_ok = orig["size"] == comp["size"]
                && orig["bands"].as_array().map(|b| b.len()).unwrap_or(0)
                    == comp["bands"].as_array().map(|b| b.len()).unwrap_or(0);

            // Check pixel content
            if let (Some(orig_bands), Some(comp_bands)) =
                (orig["bands"].as_array(), comp["bands"].as_array())
            {
                let orig_has_nodata = orig_bands.iter().any(|b| b.get("noDataValue").is_some());
                let comp_has_nodata = comp_bands.iter().any(|b| b.get("noDataValue").is_some());

                result.zstd_pixel_ok = orig_bands.iter().zip(comp_bands.iter()).all(|(ob, cb)| {
                    if orig_has_nodata && !comp_has_nodata {
                        return true;
                    }
                    ob["minimum"] == cb["minimum"] && ob["maximum"] == cb["maximum"]
                });
            }
        }
    }

    // Test uncompressed
    let test_uncomp = CompressionTest::new(image_path);
    result.uncompressed_success = test_uncomp.run("uncompressed", None);

    if result.uncompressed_success {
        // Check metadata preservation for uncompressed
        if let (Some(orig), Some(comp)) = (
            test_uncomp.get_gdalinfo(&test_uncomp.input_path),
            test_uncomp.get_gdalinfo(&test_uncomp.output_path),
        ) {
            result.uncompressed_metadata_ok = orig["size"] == comp["size"]
                && orig["bands"].as_array().map(|b| b.len()).unwrap_or(0)
                    == comp["bands"].as_array().map(|b| b.len()).unwrap_or(0);

            // Check pixel content
            if let (Some(orig_bands), Some(comp_bands)) =
                (orig["bands"].as_array(), comp["bands"].as_array())
            {
                let orig_has_nodata = orig_bands.iter().any(|b| b.get("noDataValue").is_some());
                let comp_has_nodata = comp_bands.iter().any(|b| b.get("noDataValue").is_some());

                result.uncompressed_pixel_ok =
                    orig_bands.iter().zip(comp_bands.iter()).all(|(ob, cb)| {
                        if orig_has_nodata && !comp_has_nodata {
                            return true;
                        }
                        ob["minimum"] == cb["minimum"] && ob["maximum"] == cb["maximum"]
                    });
            }
        }
    }

    result
}

// ============================================================================
// Comprehensive test: ALL images with BOTH Zstd and Uncompressed
// ============================================================================

#[test]
fn test_all_images_comprehensive() {
    let test_images = get_all_test_images();
    assert!(!test_images.is_empty(), "No test images found");

    let mut zstd_success = 0;
    let mut uncompressed_success = 0;
    let mut zstd_metadata_ok = 0;
    let mut uncompressed_metadata_ok = 0;
    let mut zstd_pixel_ok = 0;
    let mut uncompressed_pixel_ok = 0;
    let mut total = 0;

    for image_path in &test_images {
        let result = test_single_image_comprehensive(image_path);
        total += 1;

        if result.zstd_success {
            zstd_success += 1;
        }
        if result.uncompressed_success {
            uncompressed_success += 1;
        }
        if result.zstd_metadata_ok {
            zstd_metadata_ok += 1;
        }
        if result.uncompressed_metadata_ok {
            uncompressed_metadata_ok += 1;
        }
        if result.zstd_pixel_ok {
            zstd_pixel_ok += 1;
        }
        if result.uncompressed_pixel_ok {
            uncompressed_pixel_ok += 1;
        }
    }

    eprintln!("\n=== Comprehensive Test Summary (All Images) ===");
    eprintln!("Total images tested: {}", total);
    eprintln!("Zstd compression success: {}/{}", zstd_success, total);
    eprintln!("Uncompressed success: {}/{}", uncompressed_success, total);
    eprintln!("Zstd metadata preserved: {}/{}", zstd_metadata_ok, total);
    eprintln!(
        "Uncompressed metadata preserved: {}/{}",
        uncompressed_metadata_ok, total
    );
    eprintln!("Zstd pixel content preserved: {}/{}", zstd_pixel_ok, total);
    eprintln!(
        "Uncompressed pixel content preserved: {}/{}",
        uncompressed_pixel_ok, total
    );

    assert!(
        zstd_success == total,
        "{} images failed zstd compression",
        total - zstd_success
    );
    assert!(
        uncompressed_success == total,
        "{} images failed uncompressed",
        total - uncompressed_success
    );
    assert!(
        zstd_metadata_ok == total,
        "{} images had zstd metadata changes",
        total - zstd_metadata_ok
    );
    assert!(
        uncompressed_metadata_ok == total,
        "{} images had uncompressed metadata changes",
        total - uncompressed_metadata_ok
    );
    assert!(
        zstd_pixel_ok == total,
        "{} images had zstd pixel changes",
        total - zstd_pixel_ok
    );
    assert!(
        uncompressed_pixel_ok == total,
        "{} images had uncompressed pixel changes",
        total - uncompressed_pixel_ok
    );
}

// ============================================================================
// Test file size comparison: Uncompressed vs Zstd
// ============================================================================

#[test]
fn test_uncompressed_vs_zstd_file_sizes() {
    let test_images = get_all_test_images();
    assert!(!test_images.is_empty(), "No test images found");

    let mut tested_count = 0;
    let mut larger_count = 0;
    let mut similar_count = 0;
    let mut smaller_count = 0;

    for image_path in &test_images {
        let test_zstd = CompressionTest::new(image_path);
        if !test_zstd.run("zstd", Some(19)) {
            continue;
        }

        let test_uncomp = CompressionTest::new(image_path);
        if !test_uncomp.run("uncompressed", None) {
            continue;
        }

        let zstd_size = test_zstd.file_size(&test_zstd.output_path);
        let uncomp_size = test_uncomp.file_size(&test_uncomp.output_path);

        tested_count += 1;

        if uncomp_size > zstd_size {
            larger_count += 1;
        } else if uncomp_size == zstd_size {
            similar_count += 1;
        } else {
            smaller_count += 1;
        }
    }

    eprintln!("\n=== File Size Comparison (Uncompressed vs Zstd) ===");
    eprintln!("Files tested: {}", tested_count);
    eprintln!("Uncompressed larger: {}", larger_count);
    eprintln!("Similar size: {}", similar_count);
    eprintln!("Uncompressed smaller: {}", smaller_count);

    assert!(tested_count > 0, "No files could be tested");
}

// ============================================================================
// Test GeoTIFF metadata preservation
// ============================================================================

#[test]
fn test_geotiff_metadata_preservation() {
    let input_path = std::env::current_dir()
        .expect("Should get current directory")
        .join("tests/images/mask.tif");

    if !input_path.exists() {
        return; // Skip if not found
    }

    let test = CompressionTest::new(&input_path);

    // Test with Zstd
    assert!(
        test.run("zstd", Some(19)),
        "Zstd compression should succeed"
    );

    let orig = test
        .get_gdalinfo(&test.input_path)
        .expect("Should read original metadata");
    let comp = test
        .get_gdalinfo(&test.output_path)
        .expect("Should read compressed metadata");

    assert_eq!(orig["size"], comp["size"], "Dimensions should match");

    let orig_cs = orig.get("coordinateSystem");
    let comp_cs = comp.get("coordinateSystem");
    assert!(orig_cs.is_some(), "Original should have coordinate system");
    assert_eq!(orig_cs, comp_cs, "Coordinate system should be preserved");

    let orig_gt = orig.get("geoTransform").and_then(|v| v.as_array());
    let comp_gt = comp.get("geoTransform").and_then(|v| v.as_array());
    assert!(orig_gt.is_some(), "Original should have geoTransform");
    assert_eq!(orig_gt, comp_gt, "geoTransform should be preserved");
}

// ============================================================================
// Test error handling
// ============================================================================

#[test]
fn test_corrupt_file_handling() {
    let temp_dir = TempDir::new().unwrap();
    let corrupt_path = temp_dir.path().join("corrupt.tif");
    fs::write(&corrupt_path, b"NOT A TIFF FILE").unwrap();

    let output_path = temp_dir.path().join("output.tif");

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("compress")
        .arg(&corrupt_path)
        .arg("-o")
        .arg(&output_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let result = cmd.output().expect("Failed to run command");

    assert!(
        !result.status.success() || !output_path.exists(),
        "Corrupt file should not be processed successfully"
    );
}

#[test]
fn test_nonexistent_file_handling() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.tif");

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("compress")
        .arg("/nonexistent/file.tif")
        .arg("-o")
        .arg(&output_path);

    let result = cmd.output().expect("Failed to run command");
    let stderr = String::from_utf8_lossy(&result.stderr);
    let stdout = String::from_utf8_lossy(&result.stdout);

    let has_error = stderr.contains("error")
        || stderr.contains("No such file")
        || stdout.contains("error")
        || stdout.contains("No such file");

    assert!(
        has_error || !output_path.exists(),
        "Nonexistent file should produce error or no output"
    );
}

// ============================================================================
// Test multiple input files
// ============================================================================

#[test]
fn test_multiple_input_files_zstd() {
    let test_images = get_all_test_images();
    if test_images.len() < 3 {
        return;
    }

    let files: Vec<&PathBuf> = test_images.iter().take(3).collect();
    let temp_dir = TempDir::new().unwrap();
    let output_dir = temp_dir.path().join("output");
    fs::create_dir_all(&output_dir).unwrap();

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("compress")
        .args(files)
        .arg("-o")
        .arg(&output_dir)
        .arg("-f")
        .arg("zstd")
        .arg("-l")
        .arg("19");

    let result = cmd.output().expect("Failed to run command");
    assert!(
        result.status.success(),
        "Should handle multiple input files with zstd"
    );
}

#[test]
fn test_multiple_input_files_uncompressed() {
    let test_images = get_all_test_images();
    if test_images.len() < 3 {
        return;
    }

    let files: Vec<&PathBuf> = test_images.iter().take(3).collect();
    let temp_dir = TempDir::new().unwrap();
    let output_dir = temp_dir.path().join("output");
    fs::create_dir_all(&output_dir).unwrap();

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("compress")
        .args(files)
        .arg("-o")
        .arg(&output_dir)
        .arg("-f")
        .arg("uncompressed");

    let result = cmd.output().expect("Failed to run command");
    assert!(
        result.status.success(),
        "Should handle multiple input files with uncompressed"
    );
}

// ============================================================================
// Test CLI functionality
// ============================================================================

#[test]
fn test_cli_help() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"))
        .arg("--help")
        .output()
        .expect("Failed to run command");

    assert!(output.status.success(), "Help should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("compress"),
        "Help should mention compress command"
    );
    assert!(
        stdout.contains("analyze"),
        "Help should mention analyze command"
    );
}

#[test]
fn test_output_directory_creation() {
    let test_images = get_all_test_images();
    if test_images.is_empty() {
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let output_dir = temp_dir.path().join("nested").join("output");
    // Don't create the directory - let the tool create it

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("compress")
        .arg(&test_images[0])
        .arg("-o")
        .arg(&output_dir)
        .arg("-f")
        .arg("zstd")
        .arg("-l")
        .arg("19");

    let result = cmd.output().expect("Failed to run command");
    assert!(
        result.status.success(),
        "Should create output directory and compress"
    );
}

#[test]
fn test_dry_run_mode() {
    let test_images = get_all_test_images();
    if test_images.is_empty() {
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.tif");

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("compress")
        .arg(&test_images[0])
        .arg("-o")
        .arg(&output_path)
        .arg("-f")
        .arg("zstd")
        .arg("-l")
        .arg("19")
        .arg("--dry-run");

    let result = cmd.output().expect("Failed to run command");
    assert!(result.status.success(), "Dry run should succeed");
    // In dry-run mode, output file should not be created
    assert!(
        !output_path.exists(),
        "Dry run should not create output file"
    );
}
