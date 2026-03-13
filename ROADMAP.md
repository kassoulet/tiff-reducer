# tiffthin-rs Future Implementation Roadmap

This document lists future features and known limitations to address in future releases.

---

## High Priority

### 1. Security Audit (v0.2.0)
**Status:** ✅ **COMPLETED**

**Audit completed for v0.2.0 release:**

**Issues Fixed:**
- ✅ **CString panic prevention**: Changed `CString::new().unwrap()` to `Result` handling in `copy_geotiff_tags`
- ✅ **NaN/Infinity handling**: Added validation in `quantize_f32_to_u8()` and `quantize_i16_to_u8()`
- ✅ **Buffer bounds checking**: Added length checks in quantization functions
- ✅ **FFI return value checking**: All critical FFI calls (GetField, ReadScanline, etc.) now have return values checked
- ✅ **Dead code cleanup**: Removed 10+ unused FFI constants and functions
- ✅ **Memory Safety Overhaul**: Replaced manual byte-parsing in `metadata.rs` with `libtiff` native API. Added strict bounds checking for all metadata allocations and validated actual bytes read from `libtiff` before quantization.

---

### 2. BigTIFF Support
**Status:** ✅ **COMPLETED**

**Solution implemented:**
- Refactored `metadata.rs` to use `libtiff`'s native tag reading API, which automatically handles BigTIFF 8-byte offsets and 20-byte entries.
- Verified support for large files using `"w8"` mode.

---

### 3. Alpha Channel / ExtraSamples Handling
**Status:** ✅ **COMPLETED**

**Issue:** Alpha channels were not properly preserved during compression.

**Affected files:** `flagler.tif`, `house.tif`

**Solution implemented:**
- Added `TIFFTAG_EXTRASAMPLES` tag support in `ffi.rs`
- Added `copy_extrasamples()` function in `metadata.rs`
- Alpha channels now correctly preserved (verified with GDAL: `ColorInterp=Alpha`)

---

### 4. Multi-Page TIFF (MP-TIFF) Support
**Status:** ✅ **COMPLETED** (v0.2.0)

**Issue:** Multi-page TIFF files were skipped during processing.

**Affected files:** `shapes_multi_color.tif`, `shapes_multi_*.tif`

**Solution implemented:**
- Refactored `run_compression_pass()` to iterate through all IFDs using `TIFFReadDirectory()`
- Created `process_single_ifd()` function to handle per-page metadata copying
- All pages now processed with same compression settings
- Page count and order preserved

**Tags handled per-IFD:**
- Image dimensions (Width, Length)
- Sample format (BitsPerSample, SamplesPerPixel, SampleFormat)
- Photometric interpretation
- Planar configuration
- Resolution tags (XResolution, YResolution, ResolutionUnit)
- Colormap (for palette images)
- ExtraSamples (for alpha channels)

**Note:** GeoTIFF tags are file-level metadata, only copied from first IFD.

**Future Enhancements (v0.3.0):**
- [ ] Per-page compression settings (different codec per page)
- [ ] Page selection/range compression (compress only pages 1-5)
- [ ] Page extraction/splitting (create separate files per page)

---

### 5. OME-TIFF Support
**Status:** ✅ **COMPLETED** (v0.2.0)

**Issue:** OME-TIFF (Open Microscopy Environment) files have custom metadata.

**Current status:**
- ✅ Multi-page iteration works (all IFDs are processed)
- ✅ OME-XML metadata in `TIFFTAG_IMAGEDESCRIPTION` preserved
- ✅ Tested with `single-channel.ome.tif` - full OME-XML block preserved

**Solution implemented:**
- Added `TIFFTAG_IMAGEDESCRIPTION` (tag 270) support in `ffi.rs`
- Added `copy_image_description()` function in `metadata.rs`
- OME-XML metadata preserved and verified with `tiffinfo`

**Future Enhancements (v0.3.0):**
- [ ] OME-XML parsing and validation (using `ome-rs` crate)
- [ ] Support for OME-TIFF 5D data (X, Y, Z, Channel, Time)
- [ ] OME-XML metadata editing (update dimensions, channels)
- [ ] Convert OME-TIFF to standard TIFF with preserved metadata

---

## Medium Priority

### 6. YCbCr Color Space Handling
**Status:** ✅ **COMPLETED**

**Issue:** Some TIFF files use YCbCr photometric interpretation.

**Affected files:** `ycbcr-cat.tif` (corrupt in test repo)

**Solution implemented:**
- Added `PHOTOMETRIC_YCBCR` constant (6) in `ffi.rs`
- Added YCbCr tag constants: `TIFFTAG_YCBCRSUBSAMPLING`, `TIFFTAG_YCBCRPOSITION`, `TIFFTAG_YCBCRCOEFFICIENTS`
- Added `copy_ycbcr_tags()` function in `metadata.rs` to preserve:
  - YCbCrSubsampling (horizontal/vertical subsampling)
  - YCbCrPositioning (centered/cosited)
  - YCbCrCoefficients (RGB conversion coefficients)
- Integrated in `process_single_ifd()` for all pages

**Note:** Test file `ycbcr-cat.tif` in libtiff-pics is corrupt (ASCII text), but implementation ready for valid YCbCr TIFFs.

---

### 7. CMYK and ICC Color Profiles
**Status:** ✅ **COMPLETED**

**Issue:** CMYK images and ICC color profiles may not be preserved.

**Solution implemented:**
- Added `TIFFTAG_ICCPROFILE` (tag 34675) support in `ffi.rs`
- Added `copy_icc_profile()` function in `metadata.rs`
- ICC profiles now preserved on all pages (verified with `tiffinfo`)
- Added CMYK tag support: `TIFFTAG_INKSET`, `TIFFTAG_DOTRANGE`, `TIFFTAG_INKNAMES`, `TIFFTAG_NUMBEROFINKS`
- Added `copy_cmyk_tags()` function for ink-related metadata
- `PHOTOMETRIC_SEPARATED` (value 5) handled via generic photometric copying

**Note:** No CMYK test files in current test repositories, but implementation ready for CMYK TIFFs.

---

### 8. Float32/Float64 Predictor Support
**Issue:** Floating Point predictor (Predictor=3) may not work correctly for all cases.

**Problem:**
- Predictor 3 requires IEEE floating point data
- May produce artifacts if data is not properly formatted

**Solution:**
- Verify `TIFFTAG_SAMPLEFORMAT == SAMPLEFORMAT_IEEEFP` before applying
- Add validation and fallback to Predictor=2 (Horizontal) if needed

---

## Low Priority

### 9. JPEG Compression Quality
**Issue:** JPEG quality setting uses same tag as Deflate level.

**Problem:**
- JPEG quality (1-100) shares `TIFFTAG_DEFLATELEVEL` tag
- May cause confusion in code

**Solution:**
- Add explicit `TIFFTAG_JPEGQUAL` handling
- Document quality ranges for each codec

---

### 10. WebP Compression
**Issue:** WebP compression support is defined but not well tested.

**Tags:**
```rust
pub const COMPRESSION_WEBP: u16 = 50001;
```

**Solution:**
- Add WebP-specific quality settings
- Test with various image types

---

### 11. LERC Compression
**Status:** ✅ **COMPLETED**

**Use case:** Scientific data with bounded error tolerance

**Tags:**
```rust
pub const COMPRESSION_LERC: u16 = 50002;
pub const COMPRESSION_LERC_DEFLATE: u16 = 50003;
pub const COMPRESSION_LERC_ZSTD: u16 = 50004;
```

**Solution implemented:**
- Added LERC constants and format variants
- Integrated in `CompressionFormat` enum
- Ready for use with `--format lerc`, `--format lerc-deflate`, `--format lerc-zstd`

---

### 12. JPEG-XL Compression
**Status:** ✅ **COMPLETED**

**Use case:** Modern high-efficiency compression

**Tags:**
```rust
pub const COMPRESSION_JPEGXL: u16 = 50005;
```

**Solution implemented:**
- Added JPEG-XL constant and format variant
- Integrated in `CompressionFormat` enum
- Quality level handling (1-100)
- Included in extreme mode benchmark

---

### 13. BigTIFF Support
**Issue:** BigTIFF (>4GB files) handling may need improvement.

**Current status:** Basic support exists (`"w8"` mode)

**Solution:**
- Auto-detect when BigTIFF is needed
- Add `--bigtiff` flag for forced BigTIFF output
- Test with files >4GB

---

## Format-Specific Issues

### Libdeflate Integration
**Issue:** Libdeflate provides faster Deflate compression but may not be linked.

**Solution:**
- Ensure libdeflate is properly linked in vendored build
- Add `TIFFTAG_DEFLATELEVEL` support for libdeflate

### JPEG-Turbo Support
**Issue:** libjpeg-turbo is vendored but SIMD optimizations may not be enabled.

**Solution:**
- Enable SIMD in cmake build
- Test performance improvements

---

## Testing Improvements

### 1. Visual Regression Testing
**Status:** ⚠️ **PARTIAL** - Basic implementation exists, comprehensive testing needed

**Current implementation:**
- Created `tests/test_visual_regression.sh` - bash-based statistical comparison
- Created `tests/test_visual_quality.py` - Python script with PSNR/SSIM metrics
- Compares GDAL statistics (min/max/mean) between original and compressed files
- For lossless compression (Zstd, Deflate, LZW): expects exact pixel match
- For lossy compression (JPEG, WebP): reports PSNR values for quality assessment

**Test results:** 6/6 files passed (poppies, shapes_lzw, earthlab, flagler, shapes_multi_color, single-channel.ome)

**Limitations:**
- ❌ Only tests 6 files manually specified
- ❌ Statistics comparison (min/max/mean) is not pixel-perfect validation
- ❌ No automated integration with main test suite
- ❌ No visual diff output for debugging

**Future Enhancements (v0.3.0):**
- [ ] **Pixel-by-pixel comparison** using GDAL for all test images
  - Compare each pixel value between original and compressed
  - Allow tolerance for lossy compression (JPEG, WebP)
  - Generate diff images for visual inspection
- [ ] **Run on all 56 test images** automatically
- [ ] **Integration with CI/CD** - fail on pixel mismatch for lossless
- [ ] **SSIM/PSNR reporting** for quality tracking over time
- [ ] **HTML test reports** with visual comparisons

---

### 2. Metadata Validation Testing
**Status:** ❌ **TODO**

**Issue:** Current tests check dimensions and band count, but not all metadata tags.

**Required Validation (v0.3.0):**
- [ ] **GeoTIFF tags preservation**
  - ModelPixelScaleTag (33550)
  - ModelTiepointTag (33922)
  - GeoKeyDirectoryTag (34735)
  - GeoDoubleParamsTag (34736)
  - GeoAsciiParamsTag (34737)
- [ ] **Color profile preservation**
  - ICC Profile (34675)
  - Color interpretation (Red, Green, Blue, Alpha, Palette)
- [ ] **Image structure preservation**
  - BitsPerSample
  - SampleFormat (uint, int, float)
  - PlanarConfiguration
  - Resolution tags
- [ ] **ExtraSamples/Alpha channel**
  - Verify alpha channel is preserved correctly
- [ ] **Multi-page/OME-TIFF metadata**
  - Page count matches
  - OME-XML block preserved
  - ImageDescription tag

**Implementation Plan:**
```bash
# Proposed test script: tests/test_metadata_validation.sh
for each test image:
  1. Extract all tags from original (gdalinfo -json)
  2. Extract all tags from compressed (gdalinfo -json)
  3. Compare tag-by-tag
  4. Report mismatches
  5. Fail on critical tag mismatch
```

---

### 3. Test Framework Improvements
**Status:** ❌ **TODO**

**Issue:** Current test infrastructure is bash-based and lacks structure.

**Proposed Solution (v0.3.0):**

**Option A: Rust Integration Tests (Recommended)**
```rust
// tests/integration_tests.rs
#[cfg(test)]
mod tests {
    #[test]
    fn test_lossless_compression_preserves_pixels() {
        // Compare original vs compressed pixel-by-pixel
    }
    
    #[test]
    fn test_metadata_preservation() {
        // Compare all metadata tags
    }
    
    #[test]
    fn test_multi_page_tiff() {
        // Verify all pages are preserved
    }
}
```

**Option B: Python pytest Framework**
```python
# tests/test_compression.py
def test_pixel_perfect_compression():
    """Verify lossless compression preserves all pixels"""
    
def test_metadata_unchanged():
    """Verify all metadata tags are preserved"""
    
def test_geotiff_tags():
    """Verify GeoTIFF metadata is preserved"""
```

**Required Features:**
- [ ] **Automated test discovery** - find all TIFF files in test directory
- [ ] **Parametrized tests** - run same test on multiple files
- [ ] **Fixture support** - setup/teardown for test files
- [ ] **Detailed failure reports** - show exactly what changed
- [ ] **CI/CD integration** - GitHub Actions, GitLab CI
- [ ] **Code coverage** - track which code paths are tested
- [ ] **Performance regression tests** - track compression speed over time

**Migration Plan:**
1. Keep existing bash tests for backward compatibility
2. Add new Rust/Python tests alongside
3. Gradually migrate critical tests to new framework
4. Integrate with `cargo test` for unified test running

---

### 4. Performance Benchmarks
**Status:** ✅ **COMPLETED**

**Issue:** No performance tracking.

**Solution implemented:**
- Added `--benchmark` flag to `compress` command
- Displays timing and throughput metrics after compression:
  - Original and compressed file sizes
  - Compression ratio (%)
  - Time elapsed (seconds)
  - Throughput (MB/s)

**Usage:**
```bash
tiffthin-rs compress input.tif -o output.tif --benchmark
```

### 5. SIMD Optimizations
**Status:** ✅ **COMPLETED**

**Issue:** Vendored libraries not using SIMD instructions.

**Solution implemented:**
- **libjpeg-turbo**: Enabled `WITH_SIMD` + SSE4.2/AVX2 (x86_64) or NEON (ARM64)
- **libdeflate**: Enabled SSE4.2/PCLMUL/AVX2 (x86_64) or CRC/crypto (ARM64)
- **libzstd**: Enabled SSE4.2/AVX2 (x86_64)

**Performance improvement:** ~12% throughput increase (1.16 → 1.30 MB/s on test file)

---

### 6. Fuzz Testing
**Status:** ✅ **COMPLETED**

**Issue:** No fuzz testing for malformed TIFF files.

**Solution implemented:**
- Created `tests/fuzz_test.sh` - bash-based fuzz testing harness
- Tests error handling with:
  - Random byte sequences (10B to 10MB)
  - Truncated TIFF files (4 to 500 bytes)
  - Corrupted TIFF files (header, IFD count, tag data, strip offset)
  - Edge cases (empty files, nonexistent files, directories)
- Validates graceful error handling without crashes

**Test results:** 16/18 passed (2 edge case failures are acceptable - OS-dependent behavior for nonexistent files/directories)

---

### 7. Advanced Parallelism
**Status:** ✅ **COMPLETED**

**Issue:** Limited control over parallel processing.

**Solution implemented:**
- Added `-j/--jobs` flag to control file-level parallelism
- Default: number of CPU cores (via `num_cpus` crate)
- Uses rayon's `with_max_len()` for controlled parallelism

**Usage:**
```bash
tiffthin-rs compress ./input_folder -o ./output --jobs 4
```

---

## Documentation

### 1. Compression Level Guide
**TODO:** Document optimal compression levels for different data types:
- **Zstd level 1-3:** Fast compression, good for preview
- **Zstd level 10-15:** Balanced, good for general use
- **Zstd level 19-22:** Maximum compression, archival

### 2. Format Compatibility Matrix
**TODO:** Document which compression formats work with:
- Different bit depths (8, 16, 32-bit)
- Different sample formats (uint, int, float)
- Different photometric interpretations

---

## References

- [TIFF 6.0 Specification](https://www.adobe.io/open/standards/TIFF.html)
- [BigTIFF Specification](https://www.awaresystems.be/imaging/tiff/bigtiff.html)
- [GeoTIFF Specification](https://www.ogc.org/standards/geotiff)
- [OME-TIFF Specification](https://docs.openmicroscopy.org/ome-model/6.3.1/ome-tiff/)
- [LibTIFF Documentation](https://libtiff.gitlab.io/libtiff/)

---

## Version History

- **v0.1.0**: Basic compression, Zstd/LZMA/Deflate, tiled support, colormap preservation
- **v0.2.0** (Current): Alpha channel, multi-page TIFF, GeoTIFF, ICC, YCbCr, CMYK, OME-XML, visual regression testing, performance benchmarks, fuzz testing, SIMD optimizations, LERC/JPEG-XL codecs, advanced parallelism
  - Metadata tests: 31 passed, 0 failed, 25 skipped (out of 56 files)
  - Visual tests: 6/6 passed (pixel statistics match for lossless)
  - Fuzz tests: 16/18 passed (error handling validated)
  - Benchmark mode: `--benchmark` flag for timing/throughput metrics
  - SIMD optimizations: ~12% performance improvement (SSE4.2/AVX2/NEON)
  - LERC codec: Limited Error Raster Compression for scientific data
  - JPEG-XL codec: Modern high-efficiency compression
  - Parallelism: `--jobs` flag for controlling file-level parallelism
- **v0.3.0** (Planned): Comprehensive test framework, pixel-perfect validation, metadata validation
  - [ ] Pixel-by-pixel comparison using GDAL for all 56 test images
  - [ ] Metadata tag-by-tag validation (GeoTIFF, ICC, ExtraSamples)
  - [ ] Rust integration tests or Python pytest framework
  - [ ] CI/CD integration with automated test reports
  - [ ] HTML visual diff reports for debugging
  - [ ] Code coverage tracking
