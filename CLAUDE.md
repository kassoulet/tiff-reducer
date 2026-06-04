# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`tiff-reducer` is a Rust CLI tool that optimizes TIFF files using high-efficiency codecs (Zstd/LZMA/LERC/WebP/JPEG) while strictly preserving all metadata (GeoTIFF, ICC, OME-XML, CMYK/YCbCr, colormaps). It produces a single static binary with no runtime dependencies.

## Commands

### Build
```bash
cargo build --release
```
`build.rs` always builds the vendored C dependencies from source (zlib, libjpeg-turbo, libdeflate, zstd, xz/lzma, libwebp, libtiff) via git clone + cmake. The first build is slow and requires `git` and `cmake` installed. There is no system-libtiff fallback.

Distribution builds: `./scripts/build-release.sh` with `--upx` (compressed), `--musl` (fully static Linux), or `--static` (via Docker).

### Test
```bash
# Integration tests — MUST run single-threaded
cargo test --test integration_tests -- --test-threads=1

# Single test
cargo test --test integration_tests test_geotiff_metadata_preservation -- --test-threads=1

# Full suite + visual Markdown report (writes tests/README.md)
./scripts/run-tests.sh                # options: -f format, -l level, -n limit, -o output
```
Integration tests (`tests/integration_tests.rs`) run the compiled binary via `assert_cmd` against real TIFF samples in `tests/images/`. The report generator is `cargo run --bin test-report --release -- --format zstd --level 19`; it writes thumbnails/results into `tests/`. Known failing/skipped cases are documented in `tests/FAILED_TESTS_ANALYSIS.md`.

### Lint / Pre-commit
Hooks are managed with [prek](https://prek.j178.dev) (`prek.toml`): trailing whitespace, `cargo check`, `cargo clippy -- -D warnings`, `cargo fmt`, `cargo test`.
```bash
prek run --all-files
```

### Run
```bash
cargo run -- compress input.tif                      # overwrites by default
cargo run -- compress input.tif --output out.tif --format zstd --level 19
cargo run -- compress input.tif --extreme            # benchmark all formats, keep smallest
cargo run -- compress input.tif --lossy --level 85   # WebP vs JPEG, keep smallest
cargo run -- analyze input.tif                       # inspect dimensions/bit depth/compression
```

## Architecture

Small, flat module structure (~3.3k lines total):

- **`src/main.rs`** — CLI (clap derive), subcommand orchestration (`compress`, `analyze`), the core compression pipeline (`process_single_file` → `run_compression_pass`), extreme/lossy benchmarking logic, and rayon-based file-level parallelism with indicatif progress bars.
- **`src/ffi.rs`** — raw `extern "C"` bindings to libtiff (`TIFFOpen`, `TIFFGetField`/`TIFFSetField` are variadic), the `TIFFFieldInfo` struct (must match libtiff 4.x layout exactly), and all `COMPRESSION_*` / tag constants. Custom tags (GeoTIFF etc.) are registered here.
- **`src/metadata.rs`** — `clone_metadata()`: copies every supported TIFF tag from source to destination IFD. This is the heart of the "strict metadata preservation" guarantee.
- **`src/quantize.rs`** — bit-depth reduction (float32/int16 → uint8) for `--quantize`.
- **`src/bin/test-report.rs`** — standalone binary that compresses every image in `tests/images/`, verifies pixel fidelity, and generates the visual Markdown report.
- **`build.rs`** — vendored C library build orchestration (cmake).

Data flow for compression: open source TIFF → for each IFD/page → read scanlines (or tiles) via FFI → optionally quantize → clone metadata tags → write to temp output with new codec → verify → replace.

## Conventions

- Errors: `anyhow::Result` throughout.
- FFI: wrap libtiff calls in `unsafe` blocks; provide safe abstractions where possible.
- Adding support for a new TIFF tag: add the constant/registration in `src/ffi.rs` AND the cloning logic in `src/metadata.rs`.
- Parallelism is file-level only (rayon over input files); avoid intra-file parallelism.
- C dependencies are vendored for single-binary output — do not introduce system library requirements.
- Commits: do not add co-author lines (author preference).
