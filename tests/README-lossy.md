# tiff-reducer Test Report

**Generated:** 2026-05-28 23:01:40
**Mode:** Lossy (level 90)

## Summary

| Category | Count | Percentage |
|----------|-------|------------|
| ✅ Working | 3 | 60.0% |
| ❌ Failed | 2 | 40.0% |
| **Total** | **5** | **100%** |

## ❌ Failed Images

| File | Original Size | Error |
|------|---------------|-------|
| `170918_tn_neutrophil_migration_wave.ome.tif` | 2122029 bytes | Compression failed |
| `181003_multi_pos_time_course_1_MMStack.ome.tif` | 3994219 bytes | Compression failed |

## ✅ Working Images

**3 images** successfully compressed with thumbnails below:

### 12bit.cropped.rgb.tiff

| Original | Compressed |
|:---:|:---:|
| ![Original](thumbnails/12bit.cropped.rgb_orig.png) | *N/A* |

- **Original size:** 18.3 KB bytes
- **Compressed size:** 2.7 KB bytes
- **Reduction:** ⬇ 85.1%
- **Time:** 4ms

### 12bit.cropped.tiff

| Original | Compressed |
|:---:|:---:|
| ![Original](thumbnails/12bit.cropped_orig.png) | *N/A* |

- **Original size:** 6.2 KB bytes
- **Compressed size:** 1.3 KB bytes
- **Reduction:** ⬇ 79.5%
- **Time:** 2ms

### 4D-series.ome.tif

| Original | Compressed |
|:---:|:---:|
| ![Original](thumbnails/4D-series.ome_orig.png) | ![Compressed](thumbnails/4D-series.ome_comp.png) |

- **Original size:** 2.5 MB bytes
- **Compressed size:** 392.6 KB bytes
- **Reduction:** ⬇ 84.7%
- **Time:** 13ms

## Performance Metrics

- **Total execution time:** 0.78s
- **Average time per image:** 155ms
- **Throughput:** 6.5 images/sec
