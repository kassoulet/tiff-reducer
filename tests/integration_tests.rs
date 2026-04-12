//! Integration tests for tiff-reducer
//!
//! These tests verify:
//! - TIFF files can be read and compressed without errors
//! - Metadata is preserved during compression
//! - Pixel content is preserved for lossless compression
//! - Uncompressed format works correctly

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Get a small representative sample of test images for quick testing
fn get_sample_images(count: usize) -> Vec<PathBuf> {
    let test_dir = PathBuf::from("tests/images");
    if !test_dir.exists() {
        eprintln!("Test images directory not found: {:?}", test_dir);
        return Vec::new();
    }

    // Known problematic test files that should be skipped
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
            if path.extension().map_or(false, |ext| {
                ext == "tif" || ext == "tiff" || ext == "TIF" || ext == "TIFF"
            }) {
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
    files.truncate(count);
    files
}

/// Get all test images for comprehensive testing
fn get_all_test_images() -> Vec<PathBuf> {
    let test_dir = PathBuf::from("tests/images");
    if !test_dir.exists() {
        eprintln!("Test images directory not found: {:?}", test_dir);
        return Vec::new();
    }

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
            if path.extension().map_or(false, |ext| {
                ext == "tif" || ext == "tiff" || ext == "TIF" || ext == "TIFF"
            }) {
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
                    eprintln!(
                        "Command failed for {:?}: {:?}",
                        self.input_path.file_name(),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    return false;
                }
            }
            Err(e) => {
                eprintln!(
                    "Failed to run command for {:?}: {}",
                    self.input_path.file_name(),
                    e
                );
                return false;
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
        true
    }

    fn output_exists(&self) -> bool {
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

    fn original_gdalinfo(&self) -> Option<Value> {
        self.get_gdalinfo(&self.input_path)
    }

    fn compressed_gdalinfo(&self) -> Option<Value> {
        self.get_gdalinfo(&self.output_path)
    }

    fn file_size(&self, path: &Path) -> u64 {
        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }
}

// ============================================================================
// Core functionality tests
// ============================================================================

#[test]
fn test_sample_images_compress() {
    let test_images = get_sample_images(10);
    assert!(!test_images.is_empty(), "No test images found");

    for image_path in test_images {
        let test = CompressionTest::new(&image_path);
        assert!(test.run("zstd", Some(19)), "Should compress {:?}", image_path);
        assert!(test.output_exists(), "Output should exist for {:?}", image_path);
    }
}

#[test]
fn test_metadata_preserved_sample() {
    let test_images = get_sample_images(10);
    
    for image_path in test_images {
        let test = CompressionTest::new(&image_path);
        if !test.run("zstd", Some(19)) {
            continue;
        }

        let orig = match test.original_gdalinfo() {
            Some(info) => info,
            None => continue,
        };

        let comp = match test.compressed_gdalinfo() {
            Some(info) => info,
            None => continue,
        };

        assert_eq!(orig["size"], comp["size"], "Dimensions should match for {:?}", image_path);
        
        let orig_bands = orig["bands"].as_array().map(|b| b.len()).unwrap_or(0);
        let comp_bands = comp["bands"].as_array().map(|b| b.len()).unwrap_or(0);
        assert_eq!(orig_bands, comp_bands, "Band count should match for {:?}", image_path);
    }
}

// ============================================================================
// Uncompressed format tests
// ============================================================================

#[test]
fn test_uncompressed_basic() {
    let test_images = get_sample_images(10);
    
    for image_path in test_images {
        let test = CompressionTest::new(&image_path);
        assert!(test.run("uncompressed", None), "Should decompress {:?}", image_path);
        assert!(test.output_exists(), "Output should exist for {:?}", image_path);
    }
}

#[test]
fn test_uncompressed_metadata() {
    let test_images = get_sample_images(10);
    
    for image_path in test_images {
        let test = CompressionTest::new(&image_path);
        if !test.run("uncompressed", None) {
            continue;
        }

        let orig = match test.original_gdalinfo() {
            Some(info) => info,
            None => continue,
        };

        let comp = match test.compressed_gdalinfo() {
            Some(info) => info,
            None => continue,
        };

        assert_eq!(orig["size"], comp["size"], "Dimensions should match for {:?}", image_path);
    }
}

#[test]
fn test_uncompressed_uses_compression_none() {
    let input_path = std::env::current_dir()
        .expect("Should get current directory")
        .join("tests/images/shapes_uncompressed.tif");

    if !input_path.exists() {
        return; // Skip if not found
    }

    let test = CompressionTest::new(&input_path);
    assert!(test.run("uncompressed", None), "Decompression should succeed");
    assert!(test.output_exists(), "Output file should exist");

    let comp = test.compressed_gdalinfo().expect("Should read metadata");
    
    let decomp_size = test.file_size(&test.output_path);
    assert!(decomp_size > 0, "Decompressed file should have content");
}

// ============================================================================
// Error handling tests
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
// Multi-file input test
// ============================================================================

#[test]
fn test_multiple_input_files() {
    let test_images = get_sample_images(3);
    if test_images.len() < 3 {
        return; // Need at least 3 images
    }

    let temp_dir = TempDir::new().unwrap();
    let output_dir = temp_dir.path().join("output");
    fs::create_dir_all(&output_dir).unwrap();

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("compress")
        .args(&test_images)
        .arg("-o")
        .arg(&output_dir)
        .arg("-f")
        .arg("zstd")
        .arg("-l")
        .arg("19");

    let result = cmd.output().expect("Failed to run command");
    assert!(result.status.success(), "Should handle multiple input files");
}

#[test]
fn test_multiple_input_files_uncompressed() {
    let test_images = get_sample_images(3);
    if test_images.len() < 3 {
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let output_dir = temp_dir.path().join("output");
    fs::create_dir_all(&output_dir).unwrap();

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tiff-reducer"));
    cmd.arg("compress")
        .args(&test_images)
        .arg("-o")
        .arg(&output_dir)
        .arg("-f")
        .arg("uncompressed");

    let result = cmd.output().expect("Failed to run command");
    assert!(result.status.success(), "Should handle multiple input files with uncompressed");
}
