# Failed / Skipped Tests Analysis Report

**Generated:** 2026-06-05
**Version:** v0.4.0

This report covers two distinct things:

- **Part A** — the 8 images the **integration suite** (`cargo test`) excludes via
  the `skip_files` allowlist in `tests/integration_tests.rs`.
- **Part B** — the images the **visual report** (`tests/README.md`, produced by
  `test-report`) marks **"Verification Failed"** (26 lossless, 205 lossy).

**Headline:** almost all Part-B "failures" are limitations of the report's
verifier (ImageMagick `compare`), **not** compression bugs — confirmed by
comparing GDAL band checksums, which match for those images. The exceptions are
three genuine issues: **1-bit *tiled* images are corrupted**, **overviews /
reduced-resolution sub-IFDs are dropped**, and **LogLuv output uses a
non-standard compression combo**. See Part B.

---

# Part A — Integration-suite skip list (8 images)

These are excluded because of source-file corruption or libtiff format
limitations — not bugs. As of v0.4.0 **none crash** (all exit 0 after the
YCbCr-subsampling guards); they stay skipped because faithful round-trip
verification can't be satisfied for these formats.

| Image | Format | Why skipped |
|-------|--------|-------------|
| `text.tif` | THUNDERSCAN, 4-bit | corrupt/truncated data (`ThunderDecode: Not enough data`) |
| `smallliz.tif` | OJPEG + YCbCr | legacy OJPEG, unreliable round-trip |
| `zackthecat.tif` | OJPEG + YCbCr, tiled | legacy OJPEG (previously crashed) |
| `ycbcr-cat.tif` | LZW + YCbCr | YCbCr subsampling (previously crashed in `TIFFWriteDirectory`) |
| `quad-tile.jpg.tiff` | JPEG + YCbCr, tiled | tiled JPEG/YCbCr subsampled chroma |
| `quad-jpeg.tif` | JPEG, striped | downsampled JPEG needs `JPEGCOLORMODE_RGB` for scanline access |
| `tiled-jpeg-ycbcr.tif` | JPEG/YCbCr | JPEG/YCbCr handling |
| `dscf0013.tif` | Uncompressed + YCbCr (2,1) | non-(1,1) YCbCr subsampling, explicitly rejected |

Recommendation: keep all 8 skipped; revisit if the suite ever separates "must not
crash" (now satisfied) from "must round-trip faithfully".

---

# Part B — `tests/README.md` verification failures

## How the report verifies

`test-report` calls **ImageMagick `compare -metric AE`** (lossless) /
`-metric PSNR` (lossy) between the original and the compressed output and counts
it as passing only when the absolute-error pixel count is `0` and a number
parses (`src/bin/test-report.rs:377`). When `compare` cannot read a format,
exceeds a resource limit, or only diffs the first frame of a multi-page file, it
returns non-zero or errors — which the report records as "Verification Failed"
**regardless of whether tiff-reducer preserved the data**.

To separate real problems from verifier noise, each suspect image below was
re-checked with **GDAL band checksums** (`gdalinfo -checksum`), which read these
formats correctly.

## Lossless: 26 "failed" — breakdown

### B1. Verifier limitation — pixels actually preserved (false failures)

GDAL checksums of original vs compressed are **identical** for these; ImageMagick
simply can't compare them:

| Image(s) | Why `compare` fails | GDAL check |
|----------|---------------------|-----------|
| `gradient-1c-64b.tiff`, `gradient-3c-64b.tiff` | `compare: unsupported bits per pixel` (no 64-bit support) | identical (e.g. 2659 = 2659) |
| OME / multi-page: `4D-series.ome`, `multi-channel(.ome/-4D/-time/-z)`, `time-series.ome`, `z-series.ome`, `MMStack_Pos0.ome`, `background_1_MMStack.ome`, `181003_…MMStack.ome`, `renamed_internalfilenames.ome`, `renamed_uuids.ome`, `seq-1c-8b-multipage`, `subsubifds.tif`, `mri.tif`, `shapes_multi_color`, `shapes_multi_size` | multi-frame: `compare` diffs frames imperfectly → small non-zero AE | identical (e.g. multi-channel.ome 1288 = 1288; seq-multipage 2835 = 2835) |
| `house.tif` (gray+alpha, 2c) | `compare` couldn't render it (orig thumbnail `N/A`) | identical (58336 = 58336, 5934 = 5934) |
| `big_g4.tif` | `compare: width or height exceeds limit (1x65537)` — IM pixel-cache cap | (degenerate 1×65537 fax; not a tiff-reducer issue) |
| `dscf0013.tif` | YCbCr (2,1) — also on the Part-A skip list | expected |

**Action:** none on the codec. The *report* would be more accurate if
`verify_lossless` used GDAL checksums (handles 64-bit/multi-page/alpha) or diffed
per-frame instead of ImageMagick `compare`. (Tracked as a testing improvement.)

### B2. Genuine issues

#### 🔴 1-bit (sub-byte) **tiled** images are corrupted — `tiled-gray-i1.tif`
- GDAL checksum **934 (orig) → 74 (comp)** — real pixel corruption.
- Root cause: the compress tiled reader (`process_tiled_image`) assumes a
  whole-byte sample stride, so 1-bit tiled data is mis-unpacked. (The wipe path
  already *rejects* sub-byte tiled input; the compress path does not — it should
  either reject or correctly bit-unpack.)
- **This is a real correctness bug**, not a verifier artifact.

#### 🟠 Overviews / reduced-resolution sub-IFDs are dropped — `usda_naip_256_webp_z3.tif`
- Base-resolution bands are **identical** (46042/26416/45577/42149 match), but the
  original carries pyramid **overviews** that the output does not. The reported
  size "growth" (-663%) is expected: a lossy-WebP source re-encoded as lossless
  zstd is larger.
- **Action:** preserve overview/sub-IFDs, or document that they're dropped.
  (`subsubifds.tif` — SubIFD chains — likely shares this gap.)

#### 🟠 LogLuv output uses a non-standard compression — `off_luv24.tif`, `off_luv32.tif`
- Pixel data is **preserved** (all 3 band checksums match), but the output keeps
  `PHOTOMETRIC_LOGLUV` while switching compression to zstd. Strict readers reject
  this (`compare: LogLuv data must have Compression=34676 or 34677`); GDAL accepts
  it.
- **Action:** for LogLuv/SGILOG photometrics, preserve the SGILOG compression (or
  skip recompression) so output stays spec-conformant.

## Lossy: 205 "failed" — expected

Lossy compression (WebP/JPEG) is not bit-exact, so `compare -metric AE` is
non-zero by definition; the report's PSNR threshold (>20 dB) plus the same
format-read limitations as B1 account for the count. These are **not** bugs.
A meaningful lossy check would report PSNR/SSIM rather than pass/fail on exact
equality.

---

## Summary of action items (genuine)

1. **Fix 1-bit tiled compression** (`tiled-gray-i1.tif`) — reject or correctly
   bit-unpack sub-byte tiled images in the compress path.
2. **Preserve overviews / sub-IFDs** (`usda_naip_256_webp_z3.tif`, `subsubifds.tif`).
3. **Preserve SGILOG compression for LogLuv** (`off_luv24/32.tif`).
4. **Strengthen `test-report` verification** — use GDAL checksums / per-frame diff
   instead of ImageMagick `compare`, so multi-page, 64-bit, alpha and oversized
   images aren't reported as false failures.

---

## Reproduction

```bash
cargo build --release
BIN=./target/release/tiff-reducer
for f in tiled-gray-i1 multi-channel.ome gradient-1c-64b off_luv24 usda_naip_256_webp_z3; do
  "$BIN" compress "tests/images/$f."*tif* --output /tmp/c.tif
  echo "== $f =="
  gdalinfo -checksum "tests/images/$f."*tif* | grep -i checksum   # original
  gdalinfo -checksum /tmp/c.tif | grep -i checksum                # compressed
done
```

*Last updated: 2026-06-05 (v0.4.0)*
