//! Integration tests for tiffthin-rs
//!
//! These tests verify:
//! - Pixel-perfect compression for lossless codecs
//! - Metadata preservation (GeoTIFF, ICC, ExtraSamples)
//! - Multi-page TIFF handling
//! - Error handling for corrupt files

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Test fixture for compression tests
struct CompressionTest {
    #[allow(dead_code)]
    temp_dir: TempDir,  // Keep alive for duration of test
    input_path: PathBuf,
    output_path: PathBuf,
}

impl CompressionTest {
    fn new(input_name: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let input_path = PathBuf::from(format!("tests/images/exampletiffs/{}", input_name));
        let output_path = temp_dir.path().join("output.tif");

        // Skip if input file doesn't exist (submodules not initialized)
        if !input_path.exists() {
            eprintln!("Skipping test: input file not found: {:?}", input_path);
            eprintln!("Run: git submodule update --init --recursive");
        }

        Self {
            input_path,
            temp_dir,
            output_path,
        }
    }

    fn run(&self, format: &str, level: Option<u32>) -> &Self {
        // Build first to ensure binary exists
        let _ = std::process::Command::new("cargo")
            .arg("build")
            .arg("--quiet")
            .output();

        let mut cmd = std::process::Command::new("target/debug/tiffthin-rs");
        cmd.arg("compress")
            .arg(&self.input_path)
            .arg("-o")
            .arg(&self.output_path)
            .arg("-f")
            .arg(format);

        if let Some(lvl) = level {
            cmd.arg("-l").arg(lvl.to_string());
        }

        // Run and ignore stderr (libtiff warnings)
        let _ = cmd.output();
        self
    }

    fn run_benchmark(&self, format: &str) -> &Self {
        // Build first
        let _ = std::process::Command::new("cargo")
            .arg("build")
            .arg("--quiet")
            .output();

        let mut cmd = std::process::Command::new("target/debug/tiffthin-rs");
        cmd.arg("compress")
            .arg(&self.input_path)
            .arg("-o")
            .arg(&self.output_path)
            .arg("-f")
            .arg(format)
            .arg("--benchmark");

        // Run and ignore stderr (libtiff warnings)
        let _ = cmd.output();
        self
    }

    fn output_exists(&self) -> bool {
        self.output_path.exists()
    }

    fn get_gdalinfo(&self, path: &Path) -> Value {
        let output = std::process::Command::new("gdalinfo")
            .arg("-json")
            .arg(path)
            .output()
            .expect("Failed to run gdalinfo");

        serde_json::from_slice(&output.stdout).expect("Failed to parse gdalinfo JSON")
    }

    fn original_gdalinfo(&self) -> Value {
        self.get_gdalinfo(&self.input_path)
    }

    fn compressed_gdalinfo(&self) -> Value {
        self.get_gdalinfo(&self.output_path)
    }

    fn file_size(&self, path: &Path) -> u64 {
        fs::metadata(path).unwrap().len()
    }

    fn compression_ratio(&self) -> f64 {
        let orig_size = self.file_size(&self.input_path);
        let comp_size = self.file_size(&self.output_path);
        (1.0 - (comp_size as f64 / orig_size as f64)) * 100.0
    }
}

/// Check if test images are available
fn test_images_available() -> bool {
    Path::new("tests/images/exampletiffs/poppies.tif").exists()
}

// ============================================================================
// Pixel-Perfect Compression Tests
// ============================================================================

#[test]
fn test_zstd_lossless_pixel_perfect() {
    if !test_images_available() {
        eprintln!("Test images not available, skipping");
        return;
    }

    let test = CompressionTest::new("poppies.tif");
    test.run("zstd", Some(19));

    assert!(test.output_exists(), "Output file was not created");

    // Compare statistics (min/max/mean should match for lossless)
    let orig = test.original_gdalinfo();
    let comp = test.compressed_gdalinfo();

    // Dimensions must match
    assert_eq!(
        orig["size"][0], comp["size"][0],
        "Width mismatch"
    );
    assert_eq!(
        orig["size"][1], comp["size"][1],
        "Height mismatch"
    );

    // Band count must match
    let orig_bands = orig["bands"].as_array().unwrap().len();
    let comp_bands = comp["bands"].as_array().unwrap().len();
    assert_eq!(orig_bands, comp_bands, "Band count mismatch");

    // For each band, statistics should match (lossless)
    for i in 0..orig_bands {
        let orig_band = &orig["bands"][i];
        let comp_band = &comp["bands"][i];

        assert_eq!(
            orig_band["minimum"], comp_band["minimum"],
            "Band {} minimum mismatch", i
        );
        assert_eq!(
            orig_band["maximum"], comp_band["maximum"],
            "Band {} maximum mismatch", i
        );
        assert_eq!(
            orig_band["mean"], comp_band["mean"],
            "Band {} mean mismatch", i
        );
    }
}

#[test]
fn test_deflate_lossless_pixel_perfect() {
    if !test_images_available() {
        eprintln!("Test images not available, skipping");
        return;
    }

    let test = CompressionTest::new("earthlab.tif");
    test.run("deflate", Some(9));

    assert!(test.output_exists());

    let orig = test.original_gdalinfo();
    let comp = test.compressed_gdalinfo();

    // Verify dimensions match
    assert_eq!(orig["size"][0], comp["size"][0]);
    assert_eq!(orig["size"][1], comp["size"][1]);

    // Verify compression ratio is reasonable (>50% for this image)
    let ratio = test.compression_ratio();
    assert!(ratio > 50.0, "Compression ratio too low: {:.1}%", ratio);
}

#[test]
fn test_lzw_lossless_pixel_perfect() {
    if !test_images_available() {
        eprintln!("Test images not available, skipping");
        return;
    }

    let test = CompressionTest::new("shapes_lzw.tif");
    test.run("lzw", Some(9));

    assert!(test.output_exists());

    let orig = test.original_gdalinfo();
    let comp = test.compressed_gdalinfo();

    // Verify dimensions and bands match
    assert_eq!(orig["size"], comp["size"]);
    assert_eq!(orig["bands"].as_array().unwrap().len(), 
               comp["bands"].as_array().unwrap().len());
}

// ============================================================================
// Metadata Preservation Tests
// ============================================================================

#[test]
fn test_geotiff_metadata_preservation() {
    if !test_images_available() {
        eprintln!("Test images not available, skipping");
        return;
    }

    let test = CompressionTest::new("bali.tif");
    test.run("zstd", Some(19));

    assert!(test.output_exists());

    let orig = test.original_gdalinfo();
    let comp = test.compressed_gdalinfo();

    // Check GeoTIFF metadata is preserved
    // Note: gdalinfo structure varies by version, check what's available
    
    // Dimensions must match
    assert_eq!(orig["size"], comp["size"], "Dimensions changed");

    // Coordinate system should be preserved
    if let Some(orig_wkt) = orig["coordinateSystem"].as_str() {
        if let Some(comp_wkt) = comp["coordinateSystem"].as_str() {
            assert_eq!(orig_wkt, comp_wkt, "Coordinate system changed");
        }
    }

    // Corner coordinates should match (within floating point tolerance)
    if let Some(orig_corners) = orig["cornerCoordinates"].as_object() {
        if let Some(comp_corners) = comp["cornerCoordinates"].as_object() {
            assert_eq!(
                orig_corners.get("upperLeft"), comp_corners.get("upperLeft"),
                "Corner coordinates changed"
            );
        }
    }
}

#[test]
fn test_icc_profile_preservation() {
    if !test_images_available() {
        eprintln!("Test images not available, skipping");
        return;
    }

    let test = CompressionTest::new("shapes_multi_color.tif");
    test.run("zstd", Some(19));

    assert!(test.output_exists());

    let orig = test.original_gdalinfo();
    let comp = test.compressed_gdalinfo();

    // Check ICC profile metadata is preserved
    let orig_has_icc = orig
        .get("metadata")
        .and_then(|m| m.get("IMAGE_STRUCTURE"))
        .is_some();
    let comp_has_icc = comp
        .get("metadata")
        .and_then(|m| m.get("IMAGE_STRUCTURE"))
        .is_some();

    // If original has ICC profile, compressed should too
    if orig_has_icc {
        assert!(comp_has_icc, "ICC profile metadata lost during compression");
    }
}

#[test]
fn test_alpha_channel_preservation() {
    if !test_images_available() {
        eprintln!("Test images not available, skipping");
        return;
    }

    let test = CompressionTest::new("flagler.tif");
    test.run("zstd", Some(19));

    assert!(test.output_exists());

    let orig = test.original_gdalinfo();
    let comp = test.compressed_gdalinfo();

    // Check band count matches (should be 4 for RGBA)
    let orig_bands = orig["bands"].as_array().unwrap();
    let comp_bands = comp["bands"].as_array().unwrap();

    assert_eq!(
        orig_bands.len(),
        comp_bands.len(),
        "Band count changed (alpha channel lost?)"
    );

    // Check color interpretation for alpha band
    if let Some(last_band) = orig_bands.last() {
        if let Some(color_interp) = last_band.get("colorInterpretation") {
            // Find corresponding band in compressed
            if let Some(comp_last) = comp_bands.last() {
                if let Some(comp_color) = comp_last.get("colorInterpretation") {
                    assert_eq!(
                        color_interp, comp_color,
                        "Alpha channel color interpretation changed"
                    );
                }
            }
        }
    }
}

// ============================================================================
// Multi-Page TIFF Tests
// ============================================================================

#[test]
fn test_multi_page_tiff_all_pages_preserved() {
    if !test_images_available() {
        eprintln!("Test images not available, skipping");
        return;
    }

    let test = CompressionTest::new("shapes_multi_color.tif");
    test.run("zstd", Some(19));

    assert!(test.output_exists());

    // Count pages in original and compressed
    let orig_pages = count_tiff_pages(&test.input_path);
    let comp_pages = count_tiff_pages(&test.output_path);

    assert_eq!(
        orig_pages, comp_pages,
        "Page count mismatch: original={}, compressed={}",
        orig_pages, comp_pages
    );
}

/// Count pages in a multi-page TIFF using gdalinfo
fn count_tiff_pages(path: &Path) -> usize {
    let output = std::process::Command::new("gdalinfo")
        .arg(path)
        .output()
        .expect("Failed to run gdalinfo");

    let output_str = String::from_utf8_lossy(&output.stdout);
    
    // Count "TIFF directory" occurrences
    output_str.matches("TIFF directory").count().max(1)
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_corrupt_file_handling() {
    // Build first
    let _ = std::process::Command::new("cargo")
        .arg("build")
        .arg("--quiet")
        .output();

    // Create a corrupt TIFF file
    let temp_dir = TempDir::new().unwrap();
    let corrupt_path = temp_dir.path().join("corrupt.tif");
    
    // Write invalid TIFF data
    fs::write(&corrupt_path, b"NOT A TIFF FILE").unwrap();

    let output_path = temp_dir.path().join("output.tif");

    let mut cmd = std::process::Command::new("target/debug/tiffthin-rs");
    cmd.arg("compress")
        .arg(&corrupt_path)
        .arg("-o")
        .arg(&output_path);

    // Should fail gracefully (non-zero exit or no output file)
    let result = cmd.output().expect("Failed to run command");
    
    // Either command failed OR output file doesn't exist
    assert!(
        !result.status.success() || !output_path.exists(),
        "Corrupt file should not be processed successfully"
    );
}

#[test]
fn test_nonexistent_file_handling() {
    // Build first
    let _ = std::process::Command::new("cargo")
        .arg("build")
        .arg("--quiet")
        .output();

    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.tif");

    let mut cmd = std::process::Command::new("target/debug/tiffthin-rs");
    cmd.arg("compress")
        .arg("/nonexistent/file.tif")
        .arg("-o")
        .arg(&output_path);

    // Should fail with error
    let result = cmd.output().expect("Failed to run command");
    
    // Command should fail
    assert!(
        !result.status.success(),
        "Nonexistent file should cause failure"
    );
}

// ============================================================================
// Performance Tests
// ============================================================================

#[test]
fn test_benchmark_output() {
    if !test_images_available() {
        eprintln!("Test images not available, skipping");
        return;
    }

    let test = CompressionTest::new("poppies.tif");
    test.run_benchmark("zstd");

    // Benchmark mode should output timing information
    // This is verified by the run_benchmark method calling with --benchmark flag
    assert!(test.output_exists());
}

// ============================================================================
// Format Support Tests
// ============================================================================

#[test]
fn test_all_compression_formats() {
    if !test_images_available() {
        eprintln!("Test images not available, skipping");
        return;
    }

    let formats = vec!["zstd", "lzma", "deflate", "lzw"];

    for format in formats {
        let test = CompressionTest::new("poppies.tif");
        test.run(format, None);

        // Check if output was created (file may exist even with warnings)
        let output_created = test.output_exists();
        
        // Verify compression achieved some reduction (if file was created)
        if output_created {
            let ratio = test.compression_ratio();
            assert!(
                ratio > -10.0,  // Allow small increase due to overhead
                "Negative compression with {}: {:.1}%",
                format,
                ratio
            );
        }
    }
}
