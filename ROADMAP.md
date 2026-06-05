# tiff-reducer Roadmap

Forward-looking tasks and known limitations. Completed work lives in
`CHANGELOG.md` and git history.

---

## Open bugs / correctness gaps

Confirmed in the v0.4.0 verification review (detail + reproduction in
`tests/FAILED_TESTS_ANALYSIS.md`):

- 🟠 **Overviews / reduced-resolution sub-IFDs are dropped** — `usda_naip_256_webp_z3.tif`
  keeps its base bands but loses its pyramid overviews on compress; confirm and
  handle `subsubifds.tif` (SubIFD chains) too. Silent data loss.
- 🟠 **LogLuv output uses a non-standard compression** — `off_luv24/32.tif` keep
  `PHOTOMETRIC_LOGLUV` but switch to zstd, which strict readers reject (SGILOG
  requires compression 34676/34677). Preserve SGILOG, or skip recompression, for
  LogLuv/SGILOG photometrics.

---

## Security / robustness

Open items from the March 2026 audit (completed ones removed; see git history):

- [ ] **Integer overflow in tiled processing** — use `checked_mul` for
  `tile_buffer_size` / `tile_row_size` in the tiled readers (the wipe plane size
  is already guarded).
- [ ] **Null-pointer deref in `analyze_file`** — `TIFFGetField` return values
  unchecked; validate all FFI returns.
- [ ] **Use-after-free risk in metadata copying** — FFI pointers used across TIFF
  handles; copy to local buffers first.
- [ ] **Missing bounds check in tile processing** — add overflow-safe bounds
  validation on buffer access.
- [ ] **Missing `# Safety` docs** on `unsafe fn`s (verified: none present).
- [ ] **Information leakage in error messages** — sanitize internal paths.
- [ ] **Panic on `unwrap` in file processing** — `file_name()` can panic.
- [ ] **`get_sample_format` validation** — return an error on open failure
  instead of silent fallback.
- [ ] **DoS via temp-file exhaustion** — handle cleanup failures in extreme mode
  (use auto-cleanup temp dirs).
- [ ] **Unchecked `TIFFReadDirectory` return** — distinguish error from EOF.
- [ ] **Empty-file input validation**; **hardcoded path in integration tests**.

---

## Wipe command

- [ ] **Sub-byte / 12-bit wipe support** — `wipe` currently rejects 1/2/4-bit
  (non-single-channel or non-byte-aligned) and non-byte-aligned ≥8-bit (e.g.
  12-bit) images. Add bit-unpacked handling so they can be wiped while preserving
  the per-channel histogram (the compress tiled path now does packed-bit
  unpacking — reuse that technique).
- [ ] **Deeper compress/wipe dedup** — geometry, page-counting and file-loop
  scaffolding are already shared; the two tiled *decode loops* (compress's
  thread-local-handle model vs wipe's fresh-worker model) and the IFD-setup
  preambles are still duplicated. Unify them behind one abstraction.

---

## Reliability / tooling

- [ ] **Enable `JPEGCOLORMODE_RGB` on the JPEG read path** so downsampled-JPEG
  inputs (e.g. `quad-jpeg.tif`) read via scanlines instead of failing
  (`scanline oriented access is not supported for downsampled JPEG`).
- [ ] **Unify the dev Rust toolchain** (environment, outside the repo) — put
  rustup `stable` ahead of the distro rust in `PATH` so `cargo clippy` works
  everywhere; this would remove the need for `scripts/prek-env.sh`.

---

## Testing improvements

- [ ] **Stronger `test-report` verification** — replace ImageMagick
  `compare -metric AE` with GDAL band checksums (and per-frame diff for
  multi-page), so 64-bit, multi-page/OME, alpha and oversized images stop
  reporting as false failures (see `tests/FAILED_TESTS_ANALYSIS.md`).
- [ ] **ExtraSamples / alpha channel verification** — verify alpha is preserved.
- [ ] **Multi-page / OME-TIFF metadata verification** — page count matches,
  OME-XML block preserved, `ImageDescription` preserved.
- [ ] **Parametrized tests** — run the same test across multiple files.
- [ ] **Code coverage** tracking; **performance-regression** tracking.

---

## Future enhancements

- [ ] **Parallel GDAL backend** (prototype on a separate branch).
- [ ] **Difference visualization** (highlight changed pixels).
- [ ] **Report filters** — by status (failures only); **sort** by name / size /
  ratio / status.

---

## Format-specific notes / todo

- **Compression-level guide:** zstd 1–3 fast/preview, 10–15 balanced, 19–22
  archival.
- **Format compatibility matrix:** bit depths (8/16/32), sample formats
  (uint/int/float), photometric interpretations.
- **Libdeflate:** ensure it is linked in the vendored build; add
  `TIFFTAG_DEFLATELEVEL` support.
- **JPEG-Turbo:** enable SIMD in the cmake build; measure improvement.
- **BigTIFF:** auto-detect when needed; add a `--bigtiff` flag; test >4 GB.
- **JPEG quality:** explicit `TIFFTAG_JPEGQUAL` handling; document quality ranges.
- **WebP:** WebP-specific quality settings; test across image types.

---

## References

- [TIFF 6.0 Specification](https://www.adobe.io/open/standards/TIFF.html)
- [BigTIFF Specification](https://www.awaresystems.be/imaging/tiff/bigtiff.html)
- [GeoTIFF Specification](https://www.ogc.org/standards/geotiff)
- [OME-TIFF Specification](https://docs.openmicroscopy.org/ome-model/6.3.1/ome-tiff/)
- [LibTIFF Documentation](https://libtiff.gitlab.io/libtiff/)
