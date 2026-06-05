# tiff-reducer Future Implementation Roadmap

This document lists future features and known limitations to address in future releases.

## Current Release: v0.4.0

### Completed in the v0.4.0 cycle

**Wipe correctness hardening** (from the code review — all 9 findings addressed):
- Reject sub-byte (1/2/4-bit) and non-byte-aligned (≥8-bit, e.g. 12-bit) images
  in `wipe`, which the byte-level sort can't round-trip while preserving the
  per-channel histogram.
- Reject multiple inputs with a single `--output` file (was a parallel
  write/rename race) in both `wipe` and `compress`.
- Verify the histogram sample count before synthesis so a short/truncated tile
  can no longer silently zero-fill pixels.

**Cleanup / perf:** shared `count_tiff_pages` (via `TIFFNumberOfDirectories`),
`new_file_progress`/`resolve_target_output` and `TileGeometry` dedup, run-length
single-channel synthesis, in-place plane sort. (Deeper compress/wipe dedup is
still open — see Future Enhancements.)

**Tooling:** `scripts/prek-env.sh` routes prek's cargo hooks through a consistent
rustup toolchain + private target dir (works around the distro `E0514`).

### Known Bugs (found in the v0.4.0 verification review)

Surfaced by analyzing `tests/README.md` verification failures and confirmed with
GDAL band checksums. Full detail: `tests/FAILED_TESTS_ANALYSIS.md`.

- 🔴 **1-bit (sub-byte) tiled images are corrupted by `compress`** — data loss.
  `tiled-gray-i1.tif` round-trips to a different image (GDAL checksum 934 → 74).
  The compress tiled reader (`process_tiled_image`) assumes a whole-byte sample
  stride and mis-unpacks sub-byte tiled data. The wipe path already rejects
  sub-byte tiled input; the compress path must do the same or bit-unpack correctly.
- 🟠 **Overviews / reduced-resolution sub-IFDs are dropped** — `usda_naip_256_webp_z3.tif`
  (base bands preserved, pyramid overviews lost); likely also `subsubifds.tif`
  (SubIFD chains). Preserve them, or document the limitation.
- 🟠 **LogLuv output uses a non-standard compression** — `off_luv24/32.tif`: pixels
  are preserved but the output keeps `PHOTOMETRIC_LOGLUV` with zstd, which strict
  readers reject (SGILOG requires compression 34676/34677). Preserve SGILOG (or
  skip recompression) for LogLuv/SGILOG photometrics.

> Note: the visual report's verifier (ImageMagick `compare -metric AE`) produces
> many **false** failures (64-bit, multi-page/OME, alpha, oversized images) where
> the pixels are in fact identical. See the testing improvement below.

### Security Remediation (In Progress)

**Security Audit Completed:** March 2026 (18 issues identified)

#### Phase 1: Critical Fixes (Immediate - v0.4.0)
- ✅ **Path Traversal Vulnerability** — DONE: `sanitize_filename()` rejects
  `..`/absolute/separator components; applied in `resolve_target_output()`.
- ✅ **Unchecked TIFFSetField Return Value** — DONE: `metadata.rs` checks
  `TIFFSetField(...) == 0` for colormap / extrasamples / ICC and errors out.

#### Phase 2: High Severity (2 weeks - v0.4.1)
- ✅ **Buffer Overflow via Unvalidated Scanline Size** — DONE: `TIFFScanlineSize`
  is validated against the computed row size before use (3 sites in `main.rs`).

- ⚠️ **Null Pointer Dereference in analyze_file** (main.rs:183-203)
  - `TIFFGetField` return values not checked
  - Fix: Validate all FFI return values

- ⚠️ **Use-After-Free Risk in Metadata Copying** (metadata.rs:56-65)
  - FFI pointers used across TIFF handles
  - Fix: Copy data to local buffers first

- ⚠️ **Integer Overflow in Tiled Image Processing** (main.rs:800-805)
  - Multiplication can overflow for large tiles
  - Fix: Use `checked_mul()` for all size calculations
  - Partial: the wipe plane size uses `checked_mul`; the tiled `tile_buffer_size`
    computations in the compress/wipe tiled readers are still unchecked.

- ⚠️ **Missing Bounds Check in Tile Processing** (main.rs:827-832)
  - Buffer access without proper bounds validation
  - Fix: Add overflow-safe bounds checking

- ✅ **Unvalidated Compression Level Input** — DONE: levels are clamped per codec
  before reaching libtiff (zstd `1..=22`, lzma `1..=9`, webp/jpeg `1..=100`).

#### Phase 3: Medium Severity (1 month - v0.4.2)
- ⚠️ **Information Leakage in Error Messages** (main.rs:253-258)
  - Error messages may include internal paths
  - Fix: Sanitize user-facing error messages

- ⚠️ **Panic on Unwrap in File Processing** (main.rs:265)
  - `unwrap()` on `file_name()` can panic
  - Fix: Use proper error handling

- ⚠️ **Missing Validation in get_sample_format** (main.rs:508-517)
  - Silent fallback on file open failure
  - Fix: Return error when file cannot be opened

- ⚠️ **Missing Unsafe Documentation** (main.rs:569)
  - `unsafe fn` without safety documentation
  - Fix: Add safety documentation to all unsafe functions

- ⚠️ **DoS via Temp File Exhaustion** (main.rs:389-420)
  - `fs::remove_file` failures not handled in extreme mode
  - Fix: Use temp directories with automatic cleanup

- ⚠️ **Unchecked TIFFReadDirectory Return Value** (main.rs:556-559)
  - Errors not distinguished from EOF
  - Fix: Check for error conditions

#### Phase 4: Low Severity (2 months - v0.4.3)
- ℹ️ **Hardcoded Path in Integration Tests** (integration_tests.rs:89)
  - Non-portable test configuration
  - Fix: Use relative paths or environment variables

- ℹ️ **Missing Input Validation for Empty Files** (main.rs:536-540)
  - Empty files processed without validation
  - Fix: Add minimum file size check

---

## Future Enhancements
- [ ] **Parallel GDAL backend** (create separate branch for prototype)
- [ ] **Difference visualization** (highlight changed pixels)
- [ ] **Filter by status** (show only failures)
- [ ] **Sort options** (by name, size, ratio, status)

### Wipe command (follow-ups from the v0.4.0 correctness review)
- [ ] **Sub-byte / non-byte-aligned wipe support** — `wipe` currently rejects
  1/2/4-bit images that aren't single-channel + byte-aligned, and rejects
  non-byte-aligned widths ≥ 8 (e.g. 12-bit), because the byte-level sort can't
  preserve the per-sample histogram for those. Add bit-unpacked handling so they
  can be wiped while keeping the histogram guarantee.
- [ ] **Deeper compress/wipe dedup** — the v0.4.0 refactor shared page-counting,
  the file-loop scaffolding, and tile geometry (`TileGeometry`). The two *tiled
  decode loops* (compress's thread-local-handle model vs wipe's fresh-worker
  model) and the IFD-setup preambles are still duplicated; unify them behind one
  abstraction.

### Reliability / tooling
- [ ] **Enable `JPEGCOLORMODE_RGB` on the JPEG read path** so downsampled-JPEG
  inputs (e.g. `quad-jpeg.tif`) can be read via scanlines instead of failing
  (`scanline oriented access is not supported for downsampled JPEG`). See
  `tests/FAILED_TESTS_ANALYSIS.md`.

---

## Format-Specific Issues / Todo

### 1. Compression Level Guide
- **Zstd level 1-3:** Fast compression, good for preview
- **Zstd level 10-15:** Balanced, good for general use
- **Zstd level 19-22:** Maximum compression, archival

### 2. Format Compatibility Matrix
- Different bit depths (8, 16, 32-bit)
- Different sample formats (uint, int, float)
- Different photometric interpretations

### 3. Libdeflate Integration
- Ensure libdeflate is properly linked in vendored build
- Add `TIFFTAG_DEFLATELEVEL` support for libdeflate

### 4. JPEG-Turbo Support
- Enable SIMD in cmake build
- Test performance improvements

### 5. BigTIFF Support
- Auto-detect when BigTIFF is needed
- Add `--bigtiff` flag for forced BigTIFF output
- Test with files >4GB

### 6. JPEG Compression Quality
- Add explicit `TIFFTAG_JPEGQUAL` handling
- Document quality ranges for each codec

### 7. WebP Compression
- Add WebP-specific quality settings
- Test with various image types

---

## Testing Improvements

- [ ] **Parametrized tests** - run same test on multiple files
- [ ] **Code coverage** - track which code paths are tested
- [ ] **Performance regression tests** - track compression speed over time
- [ ] **ExtraSamples/Alpha channel verification** - Verify alpha channel is preserved correctly
- [ ] **Multi-page/OME-TIFF metadata verification** - Page count matches, OME-XML block preserved, ImageDescription tag preserved
- [ ] **Stronger `test-report` verification** - replace ImageMagick `compare -metric AE`
  with GDAL band checksums (and per-frame diff for multi-page), so 64-bit,
  multi-page/OME, alpha and oversized images stop reporting as false failures
  (see `tests/FAILED_TESTS_ANALYSIS.md`)

---

## References

- [TIFF 6.0 Specification](https://www.adobe.io/open/standards/TIFF.html)
- [BigTIFF Specification](https://www.awaresystems.be/imaging/tiff/bigtiff.html)
- [GeoTIFF Specification](https://www.ogc.org/standards/geotiff)
- [OME-TIFF Specification](https://docs.openmicroscopy.org/ome-model/6.3.1/ome-tiff/)
- [LibTIFF Documentation](https://libtiff.gitlab.io/libtiff/)
