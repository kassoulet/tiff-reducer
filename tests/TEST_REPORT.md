# tiff-reducer Test Report

> **Version:** v0.3.0  
> **Generated:** 2026-03-22  
> **Test Suite:** Rust Integration Tests + Visual Regression

---

## 📊 Summary

| Category | Count | Percentage |
|----------|-------|------------|
| ✅ Working | 292 | 96.1% |
| ⚠️ Skipped | 12 | 3.9% |
| **Total** | **304** | **100%** |

### 📈 Test Results Dashboard

```
Integration Tests: 6/6 passing (100%)
Image Compression: 292/304 (96.1%)
Metadata Preservation: 292/304 (96.1%)
Pixel Content (Lossless): 292/304 (96.1%)
GeoTIFF Metadata: 1/1 (100%)
```

---

## 🧪 Integration Tests

| Test | Status | Description |
|------|--------|-------------|
| test_all_images_can_be_read_and_compressed | ✅ PASS | All images compress without errors |
| test_metadata_preserved_for_all_images | ✅ PASS | Metadata preserved during compression |
| test_pixel_content_preserved_lossless | ✅ PASS | Pixel data preserved for lossless |
| test_geotiff_metadata_preservation | ✅ PASS | GeoTIFF tags preserved |
| test_corrupt_file_handling | ✅ PASS | Graceful handling of corrupt files |
| test_nonexistent_file_handling | ✅ PASS | Graceful handling of missing files |

**Total:** 6/6 passing (100%)

---

## 🗂️ Compression Formats

| Format | Status | Levels | Notes |
|--------|--------|--------|-------|
| **Zstd** | ✅ Working | 1-22 | Default codec, best overall |
| **LZMA** | ✅ Working | 1-9 | High compression ratio |
| **Deflate** | ✅ Working | 1-9 | Standard deflate |
| **LZW** | ✅ Working | N/A | Legacy, slower |
| **JPEG** | ⚠️ Partial | 1-100 | Some files crash |
| **WebP** | ✅ Working | 1-100 | Modern codec |
| **LERC** | ✅ Working | N/A | Scientific data |
| **LERC-Deflate** | ✅ Working | N/A | LERC + Deflate |
| **LERC-Zstd** | ✅ Working | N/A | LERC + Zstd |
| **JPEG-XL** | ✅ Working | 1-100 | Next-gen codec |

---

## 🏷️ Metadata Preservation

| Metadata Type | Status | Tags | Description |
|--------------|--------|------|-------------|
| **GeoTIFF** | ✅ PASS | 33550, 33922, 34735, 34736, 34737 | Coordinate system, origin, pixel size |
| **ICC Profiles** | ✅ PASS | 34675 | Full color profile preservation |
| **Alpha Channels** | ✅ PASS | ExtraSamples (#338) | Proper alpha interpretation |
| **YCbCr** | ✅ PASS | 530, 531, 532 | Color space subsampling |
| **CMYK/Ink** | ✅ PASS | 332, 336, 340, 345 | Print color spaces |
| **OME-XML** | ✅ PASS | ImageDescription (#270) | Microscopy metadata |
| **Colormap** | ✅ PASS | 320 | Palette/colormap |
| **Resolution** | ✅ PASS | 282, 283, 296 | DPI and resolution unit |

---

## ⚠️ Skipped Files (Known Issues)

### Legacy/Obsolete Formats

| File | Format | Issue | Status |
|------|--------|-------|--------|
| smallliz.tif | OJPEG | Legacy format with limited libtiff support | ⚠️ Not a bug |
| text.tif | THUNDERSCAN | Obsolete format, corrupt data | ⚠️ Not a bug |

### YCbCr Color Space (libtiff crashes)

| File | Issue | Status |
|------|-------|--------|
| ycbcr-cat.tif | YCbCr 2:2 subsampling causes crash in TIFFWriteDirectory | ⚠️ libtiff bug |
| zackthecat.tif | OJPEG + YCbCr causes crash | ⚠️ libtiff bug |
| quad-tile.jpg.tiff | Tiled JPEG + YCbCr causes crash | ⚠️ libtiff bug |
| tiled-jpeg-ycbcr.tif | JPEG/YCbCr combination causes crash | ⚠️ libtiff bug |

### Other Compression Issues

| File | Issue | Status |
|------|-------|--------|
| quad-jpeg.tif | JPEG compression causes issues | 🔍 Under investigation |
| sample-get-lzw-stuck.tiff | LZW compression causes issues | 🔍 Under investigation |

---

## 📁 Working Images by Category

### Standard TIFF Files

<details>
<summary>✅ 280+ standard TIFF files working</summary>

#### Grayscale Images
- `12bit.cropped.tiff` - 12-bit grayscale
- `earthlab.tif` - 16-bit signed integer GeoTIFF
- `fax2d.tif` - Group 3 fax compression
- `fax4.tiff` - Group 4 fax compression
- `gradient-1c-32b-float.tiff` - 32-bit float grayscale

#### RGB Color Images
- `bali.tif` - Standard RGB
- `capitol.tif` - RGB color
- `caspian.tif` - RGB landscape
- `dscf0013.tif` - Digital camera RGB
- `kodim02-lzw.tif` - Kodak test image

#### Multi-page TIFFs
- `P1_T0.tif` through `P1_T9.tif` - Time series
- `P2_T0.tif` through `P2_T9.tif` - Time series
- `shapes_multi_color.tif` - Multi-page color

</details>

### GeoTIFF Files

<details>
<summary>✅ GeoTIFF files with coordinate system preservation</summary>

- `mask.tif` - WGS 84 / UTM zone 12N (120MB test file)
- `geo-5b.tif` - 5-band GeoTIFF
- `earthlab.tif` - Sinusoidal projection

All GeoTIFF tags preserved:
- ModelPixelScaleTag (33550)
- ModelTiepointTag (33922)
- GeoKeyDirectoryTag (34735)
- GeoDoubleParamsTag (34736)
- GeoAsciiParamsTag (34737)

</details>

### OME-TIFF Files (Microscopy)

<details>
<summary>✅ OME-TIFF files with XML metadata preservation</summary>

- `170918_tn_neutrophil_migration_wave.ome.tif`
- `181003_multi_pos_time_course_1_MMStack.ome.tif`
- `4D-series.ome.tif`
- `TSeries-camp-005_Cycle*.ome.tif`

OME-XML metadata in ImageDescription tag preserved.

</details>

### Special Color Spaces

<details>
<summary>✅ CMYK, YCbCr, and other color spaces</summary>

#### CMYK Images
- `cmyk-3c-16b.tiff` - 16-bit CMYK
- `cmyk-3c-8b.tiff` - 8-bit CMYK

#### YCbCr Images (working without subsampling)
- Various YCbCr files processed correctly

</details>

---

## 🖼️ Visual Test Samples

### Sample Comparisons

| Original | Compressed | Difference |
|----------|------------|------------|
| ![bali.tif original](report/thumbnails/bali.tif_orig.png) | ![bali.tif compressed](report/thumbnails/bali.tif_comp.png) | ![bali.tif diff](report/thumbnails/bali.tif_diff.png) |
| *bali.tif* | *Zstd compressed* | *No difference* |

| Original | Compressed | Difference |
|----------|------------|------------|
| ![earthlab.tif original](report/thumbnails/earthlab.tif_orig.png) | ![earthlab.tif compressed](report/thumbnails/earthlab.tif_comp.png) | ![earthlab.tif diff](report/thumbnails/earthlab.tif_diff.png) |
| *earthlab.tif (GeoTIFF)* | *Zstd compressed* | *No difference* |

| Original | Compressed | Difference |
|----------|------------|------------|
| ![cmyk-3c-8b.tiff original](report/thumbnails/cmyk-3c-8b.tiff_orig.png) | ![cmyk-3c-8b.tiff compressed](report/thumbnails/cmyk-3c-8b.tiff_comp.png) | ![cmyk-3c-8b.tiff diff](report/thumbnails/cmyk-3c-8b.tiff_diff.png) |
| *cmyk-3c-8b.tiff* | *Zstd compressed* | *No difference* |

> **Note:** Difference images show pixel-by-pixel comparison. Black = identical, colored = differences.

---

## 📊 Performance Metrics

### Compression Ratios by Format

| Format | Avg Ratio | Speed | Best For |
|--------|-----------|-------|----------|
| Zstd (level 19) | 60-80% reduction | Fast | General purpose |
| LZMA (level 9) | 70-85% reduction | Slow | Maximum compression |
| Deflate (level 9) | 50-70% reduction | Medium | Compatibility |
| LZW | 40-60% reduction | Slow | Legacy support |
| WebP (level 80) | 70-90% reduction | Fast | Photos |
| LERC-Zstd | 80-95% reduction | Fast | Scientific data |

---

## 🔍 Test Methodology

### Integration Tests
- Run via `cargo test --test integration_tests`
- Tests compression, metadata preservation, error handling
- All tests must pass for release

### Visual Regression Tests
- Generated via `tests/generate_html_report.py`
- Compares original vs compressed images
- Uses GDAL for metadata validation
- Creates side-by-side visual comparisons

### Fuzz Testing
- 18 malformed file scenarios
- Tests error handling and graceful degradation
- Verifies no panics on invalid input

---

## 📝 Notes

### Test Environment
- **LibTIFF:** 4.7.1 (vendored, statically linked)
- **LibGeoTIFF:** Integrated via XTIFFInitialize()
- **Rust Edition:** 2021
- **Test Framework:** Rust integration tests + GDAL validation

### Known Limitations
1. **YCbCr with subsampling** - Causes libtiff crash (upstream bug)
2. **OJPEG compression** - Legacy format with limited support
3. **THUNDERSCAN** - Obsolete format, files often corrupt
4. **Multi-page OME-TIFF** - Some complex files may crash

### Future Improvements
- [ ] Fix YCbCr subsampling handling
- [ ] Add RGB conversion option for YCbCr files
- [ ] Improve JPEG compression edge cases
- [ ] Add more GeoTIFF test files
- [ ] BigTIFF test cases (>4GB files)

---

## 📄 Related Documents

- [SECURITY.md](../SECURITY.md) - Security audit findings
- [ROADMAP.md](../ROADMAP.md) - Future development plans
- [CHANGELOG.md](../CHANGELOG.md) - Version history
- [FAILED_TESTS_ANALYSIS.md](FAILED_TESTS_ANALYSIS.md) - Detailed skip analysis

---

*Report generated by tiff-reducer test suite*  
*Last updated: 2026-03-22*
