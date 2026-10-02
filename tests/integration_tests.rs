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

/// Known problematic test files, skipped by the round-trip tests. They must
/// still fail cleanly (see `test_skipped_files_fail_without_crashing`).
const SKIP_FILES: [&str; 9] = [
    "smallliz.tif",              // OJPEG compression - legacy format
    "text.tif",                  // THUNDERSCAN compression - obsolete format
    "ycbcr-cat.tif",             // YCbCr with subsampling (2,2) - rejected
    "zackthecat.tif",            // OJPEG + YCbCr - rejected
    "quad-tile.jpg.tiff",        // Tiled JPEG + YCbCr - rejected
    "quad-jpeg.tif",             // JPEG compression issues
    "tiled-jpeg-ycbcr.tif",      // JPEG/YCbCr issues
    "dscf0013.tif",              // YCbCr with subsampling (2,1) - rejected
    "sample-get-lzw-stuck.tiff", // Truncated file (tile 0: 6731 of 11457 bytes)
];

/// Get all test images for comprehensive testing
fn get_all_test_images() -> Vec<PathBuf> {
    let test_dir = PathBuf::from("tests/images");
    if !test_dir.exists() {
        eprintln!("Test images directory not found: {:?}", test_dir);
        return Vec::new();
    }

    let skip_files = SKIP_FILES;

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

#[test]
fn test_lossy_mode_benchmarking() {
    let test_images = get_all_test_images();
    if test_images.is_empty() {
        return;
    }

    let input_path = &test_images[0];
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("lossy.tif");

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("compress")
        .arg(input_path)
        .arg("-o")
        .arg(&output_path)
        .arg("--lossy")
        .arg("--level")
        .arg("90")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let result = cmd.output().expect("Failed to run lossy test");
    assert!(result.status.success(), "Lossy mode should succeed");
    assert!(output_path.exists(), "Output file should be created");
}

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
    // Assert the expected output file exists
    assert!(
        output_dir.exists(),
        "Output file should be created at {:?}",
        output_dir
    );
}

#[test]
fn test_output_directory_creation_trailing_slash() {
    let test_images = get_all_test_images();
    if test_images.is_empty() {
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let output_dir = temp_dir.path().join("newdir/");
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
        "Should create output directory (trailing slash) and compress"
    );
    // Assert the expected output file exists (input filename joined to the directory)
    let expected_output = output_dir.join(test_images[0].file_name().unwrap());
    assert!(
        expected_output.exists(),
        "Output file should be created at {:?}",
        expected_output
    );
}

#[test]
fn test_truncated_file_overwrite_fails_safely() {
    // Tests finding 11: truncated tile should be refused (not written as zeros)
    // and no temp file should be left behind in overwrite mode
    let input = PathBuf::from("tests/images/sample-get-lzw-stuck.tiff");
    if !input.exists() {
        return; // Skip if not found
    }

    let temp_dir = TempDir::new().unwrap();
    let input_copy = temp_dir.path().join("sample-get-lzw-stuck.tiff");
    fs::copy(&input, &input_copy).unwrap();
    let original_content = fs::read(&input_copy).unwrap();

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("compress")
        .arg(&input_copy)
        .arg("-f")
        .arg("zstd")
        .arg("-l")
        .arg("19")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let result = cmd.output().expect("Failed to run command");

    // Should fail with non-zero exit code
    assert!(!result.status.success(), "Truncated file should fail");

    // Original file should be unchanged
    let after_content = fs::read(&input_copy).unwrap();
    assert_eq!(
        original_content, after_content,
        "Input file should be unchanged on failure"
    );

    // No temp file should be left behind
    let temp_files: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with("tmp_tiffreducer"))
        .collect();
    assert!(
        temp_files.is_empty(),
        "No *.tmp_tiffreducer files should remain"
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

// ============================================================================
// Wipe command tests
// ============================================================================

/// Get per-band (min, max, mean, stdDev) via gdalinfo -stats.
/// Returns None if gdalinfo is unavailable.
fn get_band_stats(path: &Path) -> Option<Vec<(f64, f64, f64, f64)>> {
    let output = std::process::Command::new("gdalinfo")
        .arg("-stats")
        .arg("-json")
        .arg(path)
        .output()
        .ok()?;

    let info: Value = serde_json::from_slice(&output.stdout).ok()?;
    let bands = info["bands"].as_array()?;
    bands
        .iter()
        .map(|b| {
            Some((
                b["minimum"].as_f64()?,
                b["maximum"].as_f64()?,
                b["mean"].as_f64()?,
                b["stdDev"].as_f64()?,
            ))
        })
        .collect()
}

/// Run the wipe command on a single file
fn run_wipe(input: &Path, output: &Path) -> bool {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("wipe").arg(input).arg("-o").arg(output);

    match cmd.output() {
        Ok(out) => out.status.success() && output.exists(),
        Err(_) => false,
    }
}

#[test]
fn test_wipe_preserves_band_statistics() {
    // Cover striped/tiled, gray/RGB, contig/planar, 8/16-bit, float
    let images = [
        "flower-minisblack-08.tif",
        "flower-rgb-contig-08.tif",
        "flower-rgb-planar-08.tif",
        "flower-minisblack-16.tif",
        "cramps-tile.tif",
        "cmyk-3c-32b-float.tiff",
    ];

    for name in images {
        let input = PathBuf::from("tests/images").join(name);
        if !input.exists() {
            eprintln!("Skipping missing test image: {}", name);
            continue;
        }

        let temp_dir = TempDir::new().unwrap();
        // Copy the input so gdalinfo -stats sidecar files land in the temp dir
        let input_copy = temp_dir.path().join(name);
        fs::copy(&input, &input_copy).unwrap();
        let output = temp_dir.path().join("wiped.tif");

        assert!(run_wipe(&input_copy, &output), "Wipe failed for {}", name);

        // Wiped file must be smaller than the original
        let original_size = fs::metadata(&input_copy).map(|m| m.len()).unwrap_or(0);
        let wiped_size = fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
        assert!(
            wiped_size < original_size,
            "Wiped {} is not smaller: {} -> {} bytes",
            name,
            original_size,
            wiped_size
        );

        // Statistics must be preserved exactly (requires GDAL)
        match (get_band_stats(&input_copy), get_band_stats(&output)) {
            (Some(orig_stats), Some(wiped_stats)) => {
                assert_eq!(
                    orig_stats.len(),
                    wiped_stats.len(),
                    "Band count changed for {}",
                    name
                );
                for (i, (orig, wiped)) in orig_stats.iter().zip(wiped_stats.iter()).enumerate() {
                    assert_eq!(
                        orig, wiped,
                        "Band {} statistics changed for {}: {:?} -> {:?}",
                        i, name, orig, wiped
                    );
                }
                eprintln!(
                    "{}: stats preserved, {} -> {} bytes",
                    name, original_size, wiped_size
                );
            }
            _ => {
                eprintln!("gdalinfo unavailable, skipping stats check for {}", name);
            }
        }
    }
}

#[test]
fn test_wipe_destroys_image_content() {
    // The wiped image must NOT contain the original pixel arrangement:
    // every row of sorted data must be non-decreasing.
    let input = PathBuf::from("tests/images/flower-minisblack-08.tif");
    if !input.exists() {
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let output = temp_dir.path().join("wiped.tif");
    assert!(run_wipe(&input, &output), "Wipe failed");

    // Decode with our own binary (analyze proves it opens); for content,
    // convert via gdal if available
    let png_path = temp_dir.path().join("wiped.png");
    let convert = std::process::Command::new("gdal_translate")
        .arg("-of")
        .arg("PNG")
        .arg(&output)
        .arg(&png_path)
        .output();

    if convert.map(|o| o.status.success()).unwrap_or(false) {
        let img = image::open(&png_path).expect("Failed to open converted PNG");
        let gray = img.to_luma8();
        let pixels: Vec<u8> = gray.pixels().map(|p| p.0[0]).collect();
        let mut sorted = pixels.clone();
        sorted.sort_unstable();
        assert_eq!(
            pixels, sorted,
            "Wiped image content should be fully sorted (monotonic)"
        );
    } else {
        eprintln!("gdal_translate unavailable, skipping content check");
    }
}

#[test]
fn test_wipe_corrupt_file_handling() {
    let temp_dir = TempDir::new().unwrap();
    let corrupt_path = temp_dir.path().join("corrupt.tif");
    fs::write(&corrupt_path, b"NOT A TIFF FILE").unwrap();

    let output_path = temp_dir.path().join("output.tif");

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("wipe")
        .arg(&corrupt_path)
        .arg("-o")
        .arg(&output_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let result = cmd.output().expect("Failed to run command");

    assert!(
        !result.status.success() || !output_path.exists(),
        "Corrupt file should not be wiped successfully"
    );
}

// ============================================================================
// Output layout (--tile, --overviews) and --checksum tests
// ============================================================================

/// Run `compress` on `input` into `output` with extra arguments.
fn run_compress(input: &Path, output: &Path, extra: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"))
        .arg("compress")
        .arg(input)
        .arg("-o")
        .arg(output)
        .args(extra)
        .output()
        .expect("Failed to run command")
}

/// `gdalinfo -json`, or None if gdalinfo is unavailable.
fn gdalinfo_json(path: &Path) -> Option<Value> {
    let output = std::process::Command::new("gdalinfo")
        .arg("-json")
        .arg(path)
        .output()
        .ok()?;
    serde_json::from_slice(&output.stdout).ok()
}

#[test]
fn test_gdal_nodata_and_metadata_preserved() {
    // earthlab: GDAL NoData + metadata; caspian: GDAL metadata only. Both
    // used to crash: the ASCII GDAL tags were read as a double.
    for name in ["earthlab.tif", "caspian.tif"] {
        let input = PathBuf::from("tests/images").join(name);
        let temp_dir = TempDir::new().unwrap();
        let output = temp_dir.path().join("output.tif");

        let result = run_compress(&input, &output, &[]);
        assert!(result.status.success(), "Compression failed for {}", name);

        let (Some(orig), Some(comp)) = (gdalinfo_json(&input), gdalinfo_json(&output)) else {
            return; // gdalinfo unavailable
        };
        if name == "earthlab.tif" {
            assert!(
                !orig["bands"][0]["noDataValue"].is_null(),
                "earthlab.tif should carry a NoData value"
            );
        }
        assert_eq!(
            orig["bands"][0]["noDataValue"], comp["bands"][0]["noDataValue"],
            "NoData value should be preserved for {}",
            name
        );
        // Compare user metadata (excluding IMAGE_STRUCTURE which changes with compression)
        let orig_meta = orig["metadata"].as_object().unwrap();
        let comp_meta = comp["metadata"].as_object().unwrap();
        for (key, orig_val) in orig_meta {
            if key == "IMAGE_STRUCTURE" {
                continue;
            }
            let comp_val = comp_meta
                .get(key)
                .unwrap_or_else(|| panic!("Metadata key {} missing in compressed", key));
            assert_eq!(
                orig_val, comp_val,
                "GDAL metadata key {} should be preserved for {}",
                key, name
            );
        }
        for (o, c) in orig["bands"]
            .as_array()
            .unwrap()
            .iter()
            .zip(comp["bands"].as_array().unwrap())
        {
            assert_eq!(
                o["metadata"], c["metadata"],
                "Band metadata should be preserved for {}",
                name
            );
        }
    }
}

#[test]
fn test_tiled_output() {
    let input = PathBuf::from("tests/images/ladoga.tif");
    for (arg, expected) in [
        (Some("64x32"), [64, 32]),
        (Some("48"), [48, 48]),
        (None, [512, 512]),
    ] {
        let temp_dir = TempDir::new().unwrap();
        let output = temp_dir.path().join("output.tif");
        let mut extra = vec!["--tile"];
        extra.extend(arg);
        extra.push("--checksum");

        let result = run_compress(&input, &output, &extra);
        assert!(
            result.status.success(),
            "Tiled compression {:?} failed: {}",
            arg,
            String::from_utf8_lossy(&result.stderr)
        );

        if let Some(info) = gdalinfo_json(&output) {
            assert_eq!(
                info["bands"][0]["block"],
                serde_json::json!(expected),
                "Unexpected tile size for --tile {:?}",
                arg
            );
        }
    }
}

#[test]
fn test_invalid_tile_size_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let output = temp_dir.path().join("output.tif");
    for bad in ["100", "0x32", "abc"] {
        let result = run_compress(
            Path::new("tests/images/ladoga.tif"),
            &output,
            &["--tile", bad],
        );
        assert!(
            !result.status.success(),
            "--tile {} should be rejected",
            bad
        );
        assert!(!output.exists(), "--tile {} should not produce output", bad);
    }
}

#[test]
fn test_overviews_generated() {
    // Striped single-band with NoData, and a tiled planar RGB source
    for name in ["earthlab.tif", "shapes_lzw_tiled_planar.tif"] {
        let input = PathBuf::from("tests/images").join(name);
        let temp_dir = TempDir::new().unwrap();
        let output = temp_dir.path().join("output.tif");

        let result = run_compress(
            &input,
            &output,
            &["--overviews", "2,4", "--tile", "--checksum"],
        );
        assert!(
            result.status.success(),
            "Overview generation failed for {}: {}",
            name,
            String::from_utf8_lossy(&result.stderr)
        );

        let Some(info) = gdalinfo_json(&output) else {
            return;
        };
        let size = info["size"].as_array().unwrap();
        let (w, h) = (size[0].as_u64().unwrap(), size[1].as_u64().unwrap());
        for band in info["bands"].as_array().unwrap() {
            let overviews = band["overviews"]
                .as_array()
                .expect("Band should have overviews");
            assert_eq!(overviews.len(), 2, "Expected 2 overviews for {}", name);
            for (ovr, factor) in overviews.iter().zip([2, 4]) {
                assert_eq!(
                    ovr["size"],
                    serde_json::json!([w.div_ceil(factor), h.div_ceil(factor)]),
                    "Overview 1/{} has the wrong size for {}",
                    factor,
                    name
                );
            }
        }
    }
}

#[test]
fn test_checksum_on_varied_layouts() {
    // Contig/planar, striped/tiled, multi-IFD tiled (its reduced IFDs are
    // compared against the source pages), float with predictor
    let images = [
        "flower-rgb-contig-08.tif",
        "shapes_lzw_planar.tif",
        "shapes_lzw_tiled_planar.tif",
        "usda_naip_256_webp_z3.tif",
        "shapes_lzw_predictor3.tif",
    ];
    for name in images {
        let input = PathBuf::from("tests/images").join(name);
        let temp_dir = TempDir::new().unwrap();
        let output = temp_dir.path().join("output.tif");

        let result = run_compress(&input, &output, &["--checksum"]);
        assert!(
            result.status.success() && output.exists(),
            "--checksum failed for {}: {}",
            name,
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            String::from_utf8_lossy(&result.stdout).contains("checksum OK"),
            "--checksum should report success for {}",
            name
        );
    }
}

#[test]
fn test_failed_file_makes_exit_code_nonzero() {
    let temp_dir = TempDir::new().unwrap();
    let corrupt = temp_dir.path().join("corrupt.tif");
    fs::write(&corrupt, b"NOT A TIFF FILE").unwrap();
    let out_dir = temp_dir.path().join("out");
    fs::create_dir(&out_dir).unwrap();

    let result = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"))
        .arg("compress")
        .arg(&corrupt)
        .arg("tests/images/ladoga.tif")
        .arg("-o")
        .arg(&out_dir)
        .output()
        .expect("Failed to run command");

    assert!(!result.status.success(), "A failed file must fail the run");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("corrupt.tif"),
        "The failing file should be named on stderr"
    );
    assert!(
        out_dir.join("ladoga.tif").exists(),
        "Other files should still be processed"
    );
    assert!(
        fs::read_dir(&out_dir)
            .unwrap()
            .flatten()
            .all(|e| !e.path().to_string_lossy().ends_with("tmp_tiffreducer")),
        "No temporary files should be left behind"
    );
}

#[test]
fn test_skipped_files_fail_without_crashing() {
    // The skipped files cannot round-trip, but must be refused with an error,
    // never by a signal (quad-jpeg.tif used to segfault on ReferenceBlackWhite)
    for name in SKIP_FILES {
        let input = PathBuf::from("tests/images").join(name);
        if !input.exists() {
            continue;
        }
        let temp_dir = TempDir::new().unwrap();
        let output = temp_dir.path().join("output.tif");

        let result = run_compress(&input, &output, &[]);
        assert!(
            result.status.code().is_some(),
            "{} killed by a signal: {:?}",
            name,
            result.status
        );
        if !result.status.success() {
            assert!(!output.exists(), "{} failed but left an output", name);
        }
    }
}

#[test]
fn test_ycbcr_colorimetry_tags_preserved() {
    // YCbCrCoefficients (529) and ReferenceBlackWhite (532) are float arrays;
    // copying them used to crash. Add them to a plain file with tiffset.
    let temp_dir = TempDir::new().unwrap();
    let input = temp_dir.path().join("rbw.tif");
    fs::copy("tests/images/flower-rgb-contig-08.tif", &input).unwrap();
    for tag in [
        &["-s", "529", "0.299", "0.587", "0.114"][..],
        &["-s", "532", "0", "255", "128", "255", "128", "255"][..],
    ] {
        let set = std::process::Command::new("tiffset")
            .args(tag)
            .arg(&input)
            .output();
        if !set.is_ok_and(|o| o.status.success()) {
            return; // tiffset unavailable
        }
    }
    let output = temp_dir.path().join("output.tif");

    let result = run_compress(&input, &output, &["--checksum"]);
    assert!(
        result.status.success(),
        "Compression with colorimetry tags failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let dump = |path: &Path| {
        std::process::Command::new("tiffdump")
            .arg(path)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let (Some(orig), Some(comp)) = (dump(&input), dump(&output)) else {
        return; // tiffdump unavailable
    };
    for tag in ["YCbCrCoefficients (529)", "ReferenceBlackWhite (532)"] {
        let line = |text: &str| text.lines().find(|l| l.starts_with(tag)).map(str::to_owned);
        assert!(line(&orig).is_some(), "{} missing from the source", tag);
        assert_eq!(line(&orig), line(&comp), "{} should be preserved", tag);
    }
}

#[test]
fn test_batch_into_new_directory() {
    // A path ending in '/' is a directory, created on first write
    let temp_dir = TempDir::new().unwrap();
    let out_dir = temp_dir.path().join("batch");
    let mut out_arg = out_dir.clone().into_os_string();
    out_arg.push("/");

    let result = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"))
        .arg("compress")
        .arg("tests/images/ladoga.tif")
        .arg("tests/images/int16.tif")
        .arg("-o")
        .arg(&out_arg)
        .output()
        .expect("Failed to run command");

    assert!(
        result.status.success(),
        "Batch into a new directory failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    for name in ["ladoga.tif", "int16.tif"] {
        assert!(out_dir.join(name).exists(), "{} should be written", name);
    }
}

#[test]
fn test_dry_run_creates_no_directory() {
    let temp_dir = TempDir::new().unwrap();
    let out_dir = temp_dir.path().join("dry");
    let mut out_arg = out_dir.clone().into_os_string();
    out_arg.push("/");

    let result = run_compress(
        Path::new("tests/images/ladoga.tif"),
        Path::new(&out_arg),
        &["--dry-run"],
    );
    assert!(result.status.success(), "Dry run should succeed");
    assert!(
        !out_dir.exists(),
        "Dry run must not create the output directory"
    );
}
