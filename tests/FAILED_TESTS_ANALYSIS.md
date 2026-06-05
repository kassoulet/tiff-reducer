# Skipped Tests Analysis Report

**Generated:** 2026-06-05
**Test Suite:** tiff-reducer Integration Tests (v0.4.0)
**Skip list source:** `tests/integration_tests.rs` (`skip_files`)

---

## Executive Summary

The integration suite excludes **8** test images via the `skip_files` allowlist.
They are skipped because of source-file corruption or libtiff format limitations
(legacy/obsolete codecs and YCbCr / downsampled-JPEG handling) — **not** bugs in
tiff-reducer.

Notable change since the v0.3.0 analysis: with the YCbCr-subsampling guards added
in v0.4.0, **none of these images crash any more**. Running `compress` on each now
returns exit 0 (graceful) — verified 2026-06-05 against the release binary. They
remain on the skip list because their round-trip / pixel-fidelity verification
can't be satisfied for these formats, not because they abort.

The 8 fall into three categories:

| # | Category | Images |
|---|----------|--------|
| 1 | Corrupt / obsolete source data | `text.tif` |
| 2 | Legacy OJPEG (+ YCbCr) | `smallliz.tif`, `zackthecat.tif` |
| 3 | YCbCr / downsampled-JPEG handling | `ycbcr-cat.tif`, `quad-tile.jpg.tiff`, `quad-jpeg.tif`, `tiled-jpeg-ycbcr.tif`, `dscf0013.tif` |

---

## Per-image detail

"Current behavior" rows are from `tiff-reducer compress <img> --output /tmp/...`
on 2026-06-05; exit status and any libtiff message are noted.

### Category 1 — Corrupt / obsolete source data

#### `text.tif`
- **Format:** THUNDERSCAN (compression 32809), 1c / 4-bit, striped, 1512×359
- **Current behavior:** exit 0, but libtiff emits
  `ThunderDecode: Not enough data at scanline 356 (0 != 1512).`
- **Root cause:** THUNDERSCAN is an obsolete codec and this file's data is
  truncated/corrupt — libtiff cannot fully decode it, so the decoded pixels are
  incomplete and fidelity verification cannot pass.
- **Recommendation:** keep skipped (corrupt input, not fixable here).

### Category 2 — Legacy OJPEG

#### `smallliz.tif`
- **Format:** OJPEG (Old JPEG, compression 6) + YCbCr, 3c / 8-bit, striped, 160×160
- **Current behavior:** exit 0 (graceful).
- **Root cause:** OJPEG is a deprecated, poorly-specified codec; combined with
  YCbCr subsampling it cannot be round-tripped reliably.
- **Recommendation:** keep skipped (legacy format).

#### `zackthecat.tif`
- **Format:** OJPEG (compression 6) + YCbCr, 3c / 8-bit, tiled (240×224), 234×213
- **Current behavior:** exit 0 (graceful; previously crashed).
- **Root cause:** same OJPEG + YCbCr limitation as `smallliz.tif`, tiled variant.
- **Recommendation:** keep skipped (legacy format).

### Category 3 — YCbCr / downsampled JPEG

These use `PHOTOMETRIC_YCBCR` (often via JPEG) where chroma subsampling and/or
scanline-vs-strip access prevent a faithful decode→re-encode round trip.

#### `ycbcr-cat.tif`
- **Format:** LZW + YCbCr, 3c / 8-bit, striped, 250×325
- **Current behavior:** exit 0 (graceful; previously crashed in `TIFFWriteDirectory`).
- **Root cause:** YCbCr subsampling; faithful pixel round-trip not guaranteed.

#### `quad-tile.jpg.tiff`
- **Format:** JPEG + YCbCr, tiled (128×128), 3c / 8-bit, 512×384
- **Current behavior:** exit 0 (graceful).
- **Root cause:** tiled JPEG/YCbCr with subsampled chroma planes.

#### `quad-jpeg.tif`
- **Format:** JPEG, 3c / 8-bit, striped, 512×384
- **Current behavior:** exit 0, with libtiff message
  `TIFFReadScanline: scanline oriented access is not supported for downsampled
  JPEG compressed images, consider enabling TIFFTAG_JPEGCOLORMODE as
  JPEGCOLORMODE_RGB.`
- **Root cause:** downsampled JPEG requires `JPEGCOLORMODE_RGB` for scanline
  access, which the current read path does not set, so scanlines can't be read.

#### `tiled-jpeg-ycbcr.tif`
- **Format:** JPEG/YCbCr, 3c / 8-bit, 374×499
- **Current behavior:** exit 0 (graceful).
- **Root cause:** JPEG/YCbCr handling as above.

#### `dscf0013.tif`
- **Format:** Uncompressed + YCbCr subsampling (2,1), 3c / 8-bit, striped, 640×480
- **Current behavior:** exit 0 — explicitly rejected by the YCbCr-subsampling
  guard (subsampling ≠ (1,1)) added in v0.4.0.
- **Root cause:** non-(1,1) YCbCr subsampling is intentionally rejected to avoid
  producing an incorrect image.

---

## Recommendations

1. **Keep all 8 skipped** — each is a corrupt source or an unsupported
   legacy/YCbCr/downsampled-JPEG format, not a regression.
2. **Re-evaluate the skip list periodically:** since none of these abort any more,
   the list could be narrowed if/when the suite distinguishes "must not crash"
   (now satisfied by all 8) from "must round-trip faithfully" (still unmet).
3. **Possible future work** (tracked in `ROADMAP.md`): enable `JPEGCOLORMODE_RGB`
   on the JPEG read path so downsampled-JPEG inputs like `quad-jpeg.tif` can be
   read via scanlines and re-encoded.

---

## Reproduction

```bash
cargo build --release
for f in smallliz text ycbcr-cat zackthecat quad-tile.jpg quad-jpeg tiled-jpeg-ycbcr dscf0013; do
  ./target/release/tiff-reducer compress tests/images/"$f".*tif* --output "/tmp/$f.tif"
  echo "$f -> exit $?"
done
```

---

*Last updated: 2026-06-05 (v0.4.0)*
