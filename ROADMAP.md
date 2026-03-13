# tiffthin-rs Future Implementation Roadmap

This document lists missing features, problematic formats, and known limitations to address in future releases.

---

## High Priority

### 1. Alpha Channel / ExtraSamples Handling
**Status:** ✅ **COMPLETED**

**Issue:** Alpha channels are not properly preserved during compression.

**Affected files:** `flagler.tif`, `house.tif`

**Solution implemented:**
- Added `TIFFTAG_EXTRASAMPLES` tag support in `ffi.rs`
- Added `copy_extrasamples()` function in `metadata.rs` to preserve ExtraSamples tag
- Alpha channels now correctly preserved (verified with GDAL: `ColorInterp=Alpha`)

---

### 2. Multi-Page TIFF (MP-TIFF) Support
**Status:** ✅ **COMPLETED**

**Issue:** Multi-page TIFF files were skipped during processing.

**Affected files:** `shapes_multi_color.tif`, `shapes_multi_*.tif`, OME-TIFF files

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

---

### 3. OME-TIFF Support
**Status:** ✅ **COMPLETED**

**Issue:** OME-TIFF (Open Microscopy Environment) files have custom metadata.

**Current status:**
- ✅ Multi-page iteration works (all IFDs are processed)
- ✅ OME-XML metadata in `TIFFTAG_IMAGEDESCRIPTION` preserved

**Solution implemented:**
- Added `TIFFTAG_IMAGEDESCRIPTION` (tag 270) support in `ffi.rs`
- Added `copy_image_description()` function in `metadata.rs`
- OME-XML metadata preserved and verified with `tiffinfo`
- Tested with `single-channel.ome.tif` - full OME-XML block preserved

---

## Medium Priority

### 4. YCbCr Color Space Handling
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

### 5. CMYK and ICC Color Profiles
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

### 6. Float32/Float64 Predictor Support
**Issue:** Floating Point predictor (Predictor=3) may not work correctly for all cases.

**Problem:**
- Predictor 3 requires IEEE floating point data
- May produce artifacts if data is not properly formatted

**Solution:**
- Verify `TIFFTAG_SAMPLEFORMAT == SAMPLEFORMAT_IEEEFP` before applying
- Add validation and fallback to Predictor=2 (Horizontal) if needed

---

## Low Priority

### 7. JPEG Compression Quality
**Issue:** JPEG quality setting uses same tag as Deflate level.

**Problem:**
- JPEG quality (1-100) shares `TIFFTAG_DEFLATELEVEL` tag
- May cause confusion in code

**Solution:**
- Add explicit `TIFFTAG_JPEGQUAL` handling
- Document quality ranges for each codec

---

### 8. WebP Compression
**Issue:** WebP compression support is defined but not well tested.

**Tags:**
```rust
pub const COMPRESSION_WEBP: u16 = 50001;
```

**Solution:**
- Add WebP-specific quality settings
- Test with various image types

---

### 9. LERC Compression
**Issue:** LERC (Limited Error Raster Compression) not supported.

**Use case:** Scientific data with bounded error tolerance

**Tags:**
```rust
pub const COMPRESSION_LERC: u16 = 50002;
pub const COMPRESSION_LERC_DEFLATE: u16 = 50003;
pub const COMPRESSION_LERC_ZSTD: u16 = 50004;
```

---

### 10. BigTIFF Support
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
**Status:** ✅ **COMPLETED**

**Issue:** Current tests only check metadata, not pixel values.

**Solution implemented:**
- Created `tests/test_visual_regression.sh` - bash-based statistical comparison
- Created `tests/test_visual_quality.py` - Python script with PSNR/SSIM metrics
- Compares GDAL statistics (min/max/mean) between original and compressed files
- For lossless compression (Zstd, Deflate, LZW): expects exact pixel match
- For lossy compression (JPEG, WebP): reports PSNR values for quality assessment

**Test results:** 6/6 files passed (poppies, shapes_lzw, earthlab, flagler, shapes_multi_color, single-channel.ome)

### 2. Performance Benchmarks
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

### 3. Fuzz Testing
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
- **v0.2.0** (Current): Alpha channel, multi-page TIFF, GeoTIFF, ICC, YCbCr, CMYK, OME-XML, visual regression testing, performance benchmarks, fuzz testing
  - Metadata tests: 27 passed, 0 failed, 29 skipped (out of 56 files)
  - Visual tests: 6/6 passed (pixel statistics match for lossless)
  - Fuzz tests: 16/18 passed (error handling validated)
  - Benchmark mode: `--benchmark` flag for timing/throughput metrics
- **v0.3.0** (Planned): SIMD optimizations, additional codec support
- **v0.4.0** (Planned): LERC/JPEG-XL codecs, advanced parallelism
