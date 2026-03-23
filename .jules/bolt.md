# Bolt's Journal - TIFF Reducer Performance Optimization

## 2025-03-23 - Predictor Hardcoding Bottleneck
**Learning:** Found that TIFF predictors (Horizontal and Floating Point) were hardcoded to `PREDICTOR_NONE` (1) in `src/main.rs`. These predictors are essential for maximizing the efficiency of LZW, Deflate, Zstd, and LZMA compression, as they transform pixel data into a more compressible format (e.g., differences between neighboring pixels).

**Action:** Re-enabled predictor support with safety checks for bit depth and sample format compatibility. This should improve compression ratios by 15-30% on many images without significant CPU overhead.
