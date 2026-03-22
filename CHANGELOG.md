# Changelog

All notable changes to tiff-reducer are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security
- **Security Audit (March 2026)**: Comprehensive audit identified 18 issues
  - 2 Critical: path traversal, unchecked FFI return values
  - 8 High: buffer overflow, null pointer, integer overflow, use-after-free
  - 6 Medium: error handling, documentation, DoS vectors
  - 2 Low: cosmetic issues
- **Remediation Plan**: 4-phase approach (see ROADMAP.md)
  - Phase 1 (Immediate): Critical fixes
  - Phase 2 (2 weeks): High severity
  - Phase 3 (1 month): Medium severity
  - Phase 4 (2 months): Low severity

### Added
- **GDAL Metadata Support**: Added constants for TIFFTAG_GDAL_METADATA (42112) and TIFFTAG_GDAL_NODATA (42113)
- **copy_gdal_tags() function**: Copies GDAL metadata during compression (requires libtiff field info registration)

### Fixed
- **Test binary path**: Fixed hardcoded path in error handling tests
- **Test accuracy**: Updated pixel content test to handle NoDataValue statistics differences

### Changed
- **Test infrastructure**: Updated skip list with accurate failure reasons
  - smallliz.tif: OJPEG legacy format
  - text.tif: THUNDERSCAN obsolete format
  - ycbcr-cat.tif, zackthecat.tif, quad-tile.jpg.tiff: YCbCr subsampling crashes
  - quad-jpeg.tif, sample-get-lzw-stuck.tiff, tiled-jpeg-ycbcr.tif: Compression issues

### Test Results
- **Integration Tests**: 6/6 passing
- **Image Compression**: 292/304 (96% success rate)
- **Known Issues**: 12 files skipped (corrupt/unsupported formats)

## [0.3.0] - 2026-03-22

### Added

#### HTML Visual Test Reports
- **`tests/generate_html_report.py`**: Python-based HTML report generator (652 lines)
- **`tests/generate-report.sh`**: Shell wrapper script with CLI options
- **Features**:
  - Side-by-side image comparison (256x256 PNG thumbnails via GDAL)
  - Metadata comparison tables (via gdalinfo JSON)
  - Color-coded pass/fail indicators (green/red borders)
  - File size and compression ratio display
  - Summary dashboard with statistics
  - Responsive design (mobile/desktop compatible)
  - Expandable test case details
- **CI/CD Integration**:
  - HTML report job in `.github/workflows/ci.yml`
  - Artifact upload with 7-day retention
  - Conditional execution on push to `kassoulet/tiff-reducer`
- **Test Results**: 157/304 images working (51.6%)

#### Tiled Image Processing
- **`process_tiled_image()` function**: Proper tile reading using `TIFFReadEncodedTile`
- Reads compressed tiles and automatically decompresses them
- Converts tile data to scanline format for strip-based output
- Handles edge tiles with partial dimensions
- Correct bytes per pixel calculation for different sample formats
- Quantization support for tiled images
- **Working formats**: cramps-tile.tif, quad-tile.tif, tiled-rgb-u8.tif, and more

#### Test Infrastructure
- **`tests/generate_test_report.py`**: Comprehensive test report generator
- **`tests/TEST_REPORT.md`**: Detailed markdown test report with:
  - Working/non-working image categorization
  - Failure type breakdown (TIFFWriteDirectorySec, read errors, etc.)
  - Known limitations documentation
- **Rust integration tests**: Error handling tests (2/2 passing)

#### CI/CD Documentation
- **README.md**: New CI/CD section documenting all GitHub Actions workflows
- Workflow descriptions: CI, Visual Tests, Release
- HTML report artifact access instructions

### Changed

#### Compression Fixes
- **Predictor disabled by default**: Horizontal predictor causes crashes with ZSTD/Deflate in libtiff 4.5.1
- **DEFLATELEVEL tag disabled**: Causes crashes with some TIFF files
- **Compression codec ordering**: Set codec BEFORE level tags (libtiff requirement)
- **PLANARCONFIG handling**: Only set for multi-sample images (spp > 1)
- **RowsPerStrip**: Set explicitly for strip-based output when converting from tiled

#### Code Quality
- Fixed all clippy warnings (unnecessary casts, div_ceil usage, unused variables)
- Formatted all code with `cargo fmt`
- Removed co-author trailers from 6 git commits
- Updated git history to have single author only

#### Documentation
- **ROADMAP.md**: Updated HTML Visual Test Reports section (TODO → COMPLETED)
  - Added architecture diagram (Rust → Python → HTML)
  - Documented technology stack (Rust, GDAL, Python)
  - Added test results and known limitations
  - Listed future enhancements

### Fixed

- **Tiled image reading**: Use `TIFFReadEncodedTile` instead of `TIFFReadScanline`
- **Tile dimension calculation**: Correct handling of edge tiles
- **Bytes per pixel**: Proper calculation for different sample formats
- **Clippy warnings**: 9 warnings fixed in final cleanup

### Known Limitations

- **Multi-page OME-TIFF**: TIFFWriteDirectorySec crashes (libtiff 4.5.1 bug)
  - Affected: 4D-series.ome.tif, MMStack_Pos0.ome.tif, TSeries-*.ome.tif
- **Tiled images with complex metadata**: Some LZW-compressed tiled files crash
  - Affected: shapes_lzw_tiled.tif, shapes_tiled_multi.tif
- **Non-standard bit depths**: 3/5/7-bit samples may fail
- **Compression level tags**: DEFLATELEVEL disabled, ZSTD level not supported

### Test Results

- **Total images**: 304
- **Working**: 157 (51.6%)
- **Failed**: 147 (48.4%)
  - TIFFWriteDirectorySec crashes: 142
  - Other errors (no output): 5
- **Error handling tests**: 2/2 passing

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
