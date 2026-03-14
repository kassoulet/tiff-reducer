# Changelog

All notable changes to tiff-reducer are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-03-14

### Added

#### Testing Infrastructure
- **Rust integration tests**: Comprehensive test framework in `tests/integration_tests.rs`
  - Compression tests for all formats
  - Metadata preservation validation
  - Pixel-perfect comparison for lossless compression
  - Error handling tests (corrupt files, nonexistent files)
  - 11/11 tests passing
- **CI/CD workflow updates**:
  - GitHub Actions updated to Node.js 24 compatible versions
  - `actions/checkout@v4`, `actions/upload-artifact@v4`
  - `softprops/action-gh-release@v1` for releases
  - All CI checks pass: build, fmt, clippy

#### Command-Line Features
- **Dry-run mode**: `--dry-run` flag for benchmarking without writing to disk

### Changed

#### Code Quality
- Fixed all clippy warnings (`too_many_arguments` for FFI functions)
- Added `#![allow(dead_code)]` for intentionally kept FFI bindings
- Formatted all code with `cargo fmt`
- Removed co-author lines from git commits

#### Error Handling
- Improved predictor validation for non-standard bit depths
- Better handling of missing/invalid TIFF tags (SamplesPerPixel, Photometric, PlanarConfig)
- Compression level tag ordering fixed to prevent crashes

### Fixed

- **ZSTD compression level**: Disabled level setting (not supported in libtiff 4.5.1)
- **Predictor validation**: Only apply horizontal predictor for 8/16/32-bit samples
- **Multi-page TIFF handling**: Fixed crash with OME-TIFF files
- **Tiled image processing**: Improved scanline-based reading for tiled images
- **CI workflow**: Fixed Rust toolchain installation and binary paths

### Test Results

- **Integration tests**: 11 passed, 0 failed
- **Image compression**: ~54% success rate (164/304 images)
  - Working: Standard bit depths (8/16/32), single-page, strip-based images
  - Known issues: Multi-page OME-TIFF, some tiled images, non-standard bit depths

## [0.2.0] - 2026-03-13

### Added

#### Compression Codecs
- **LERC codec**: Limited Error Raster Compression for scientific data (tags 50002-50004)
- **JPEG-XL codec**: Modern high-efficiency compression (tag 50005)
- Compression level support for all codecs

#### Metadata Preservation
- **Alpha channels**: ExtraSamples tag (#338) preservation
- **ICC profiles**: Full color profile preservation (tag 34675)
- **YCbCr color space**: Subsampling, positioning, and coefficients
- **CMYK support**: InkSet, DotRange, InkNames, NumberOfInks tags
- **OME-XML**: ImageDescription tag for microscopy data

#### Features
- **Multi-page TIFF**: Full IFD iteration for all pages
- **Parallel processing**: `--jobs` flag for controlling file-level parallelism
- **Benchmark mode**: `--benchmark` flag for timing/throughput metrics
- **Visual regression testing**: GDAL-based pixel statistics comparison
- **Fuzz testing**: 18 malformed file scenarios for error handling

#### Performance
- **SIMD optimizations**: SSE4.2/AVX2 (x86_64) and NEON (ARM64)
  - libjpeg-turbo: WITH_SIMD enabled
  - libdeflate: SSE4.2/PCLMUL/AVX2 optimizations
  - libzstd: SSE4.2/AVX2 optimizations
  - ~12% throughput improvement

### Changed

#### Security Improvements
- Fixed potential panic in `copy_geotiff_tags` (CString::unwrap → Result)
- Added NaN/infinity handling in quantization functions
- Added buffer bounds checking in quantization
- Improved error messages for invalid inputs

#### Code Quality
- Removed unused FFI constants and functions
- Added `#[allow(dead_code)]` for intentionally kept constants
- Fixed all compiler warnings
- Added comprehensive documentation

### Fixed

- Alpha channel interpretation (was "Undefined", now "Alpha")
- Multi-page TIFF first page handling
- SampleFormat tag handling for subsequent pages

### Test Results

- **Metadata tests**: 27 passed, 0 failed, 29 skipped (56 files)
- **Visual tests**: 6/6 passed (lossless pixel-perfect compression)
- **Fuzz tests**: 16/18 passed (graceful error handling)

## [0.1.0] - 2026-03-12

### Added

- Initial release
- Core compression: Zstd, LZMA, Deflate, LZW, JPEG, WebP
- Predictor support: None, Horizontal, Floating Point
- GeoTIFF metadata preservation (tags 33550, 33922, 34735, 34736, 34737)
- Tiled TIFF support
- Colormap/palette preservation
- Quantization (float32/int16 to uint8)
- BigTIFF support
- Vendored build option

[Unreleased]: https://github.com/kassoulet/tiff-reducer/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/kassoulet/tiff-reducer/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/kassoulet/tiff-reducer/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/kassoulet/tiff-reducer/releases/tag/v0.1.0
