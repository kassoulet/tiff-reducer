# tiff-reducer Test Report

**Generated:** 2026-05-28 23:24:46

## Summary

- [Lossless Report](#lossless-report): 5 working, 0 failed
- [Lossy Report](#lossy-report): 3 working, 2 failed

<a id="lossless-report"></a>
## Lossless Report

### Summary

| Category | Count | Percentage |
|----------|-------|------------|
| ✅ Working | 5 | 100.0% |
| ❌ Failed | 0 | 0.0% |
| **Total** | **5** | **100%** |

### ✅ Working Images

| Original | Compressed | Details |
|:---:|:---:|:---:|
| ![Original](thumbnails/12bit.cropped.rgb_orig.png) | ![Compressed](thumbnails/12bit.cropped.rgb_comp.png) | **File:** `12bit.cropped.rgb.tiff`<br>**Codec:** zstd (lvl 19)<br>**Size:** 18.3 KB → 3.1 KB<br>**Red:** 82.8%<br>**Time:** 8ms |
| ![Original](thumbnails/12bit.cropped_orig.png) | ![Compressed](thumbnails/12bit.cropped_comp.png) | **File:** `12bit.cropped.tiff`<br>**Codec:** zstd (lvl 19)<br>**Size:** 6.2 KB → 2.3 KB<br>**Red:** 63.7%<br>**Time:** 8ms |
| ![Original](thumbnails/170918_tn_neutrophil_migration_wave.ome_orig.png) | ![Compressed](thumbnails/170918_tn_neutrophil_migration_wave.ome_comp.png) | **File:** `170918_tn_neutrophil_migration_wave.ome.tif`<br>**Codec:** zstd (lvl 19)<br>**Size:** 2.0 MB → 1.3 MB<br>**Red:** 35.1%<br>**Time:** 49ms |
| ![Original](thumbnails/181003_multi_pos_time_course_1_MMStack.ome_orig.png) | ![Compressed](thumbnails/181003_multi_pos_time_course_1_MMStack.ome_comp.png) | **File:** `181003_multi_pos_time_course_1_MMStack.ome.tif`<br>**Codec:** zstd (lvl 19)<br>**Size:** 3.8 MB → 2.2 MB<br>**Red:** 41.3%<br>**Time:** 26ms |
| ![Original](thumbnails/4D-series.ome_orig.png) | ![Compressed](thumbnails/4D-series.ome_comp.png) | **File:** `4D-series.ome.tif`<br>**Codec:** zstd (lvl 19)<br>**Size:** 2.5 MB → 81.9 KB<br>**Red:** 96.8%<br>**Time:** 43ms |

<a id="lossy-report"></a>
## Lossy Report

### Summary

| Category | Count | Percentage |
|----------|-------|------------|
| ✅ Working | 3 | 60.0% |
| ❌ Failed | 2 | 40.0% |
| **Total** | **5** | **100%** |

### ❌ Failed Images

| File | Original Size | Error |
|------|---------------|-------|
| `170918_tn_neutrophil_migration_wave.ome.tif` | 2122029 bytes | Compression failed |
| `181003_multi_pos_time_course_1_MMStack.ome.tif` | 3994219 bytes | Compression failed |

### ✅ Working Images

| Original | Compressed | Details |
|:---:|:---:|:---:|
| ![Original](thumbnails/12bit.cropped.rgb_orig.png) | *N/A* | **File:** `12bit.cropped.rgb.tiff`<br>**Codec:** zstd (lvl 19)<br>**Size:** 18.3 KB → 715 B<br>**Red:** 96.2%<br>**Time:** 2ms |
| ![Original](thumbnails/12bit.cropped_orig.png) | *N/A* | **File:** `12bit.cropped.tiff`<br>**Codec:** zstd (lvl 19)<br>**Size:** 6.2 KB → 639 B<br>**Red:** 90.0%<br>**Time:** 2ms |
| ![Original](thumbnails/4D-series.ome_orig.png) | ![Compressed](thumbnails/4D-series.ome_comp.png) | **File:** `4D-series.ome.tif`<br>**Codec:** zstd (lvl 19)<br>**Size:** 2.5 MB → 138.7 KB<br>**Red:** 94.6%<br>**Time:** 11ms |
