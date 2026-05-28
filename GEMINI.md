# GEMINI.md - tiff-reducer 🐘

This file provides instructional context for Gemini CLI when working on the `tiff-reducer` project.

## Project Overview

`tiff-reducer` is a high-performance CLI tool written in Rust designed to optimize TIFF files while strictly preserving all metadata (GeoTIFF, ICC profiles, OME-XML, CMYK/YCbCr parameters, etc.).

### Core Technologies
- **Language:** Rust (2021 edition)
- **TIFF Handling:** `libtiff` via custom FFI bindings (`src/ffi.rs`)
- **CLI Framework:** `clap` (derive API)
- **Parallelism:** `rayon` for concurrent file processing
- **Progress UI:** `indicatif` for progress bars and multi-progress management
- **Build System:** `cargo` with `cmake` (for vendored libtiff)

### Architecture
- **CLI Entry:** `src/main.rs` manages subcommands and orchestration.
- **FFI Layer:** `src/ffi.rs` defines raw bindings to `libtiff`.
- **Metadata Logic:** `src/metadata.rs` handles the complex task of cloning and preserving various TIFF tags.
- **Quantization:** `src/quantize.rs` provides utilities for bit depth reduction (e.g., float32 -> uint8).
- **Subcommands:**
  - `compress`: Main optimization engine. Supports multiple codecs, "extreme" benchmarking, and directory processing.
  - `analyze`: Utility to inspect TIFF dimensions, channels, bit depth, and current compression.

---

## Building and Running

### Development Build
```bash
cargo build
```

### Release Build (Vendored)
Recommended for distribution as it bundles all compression libraries.
```bash
cargo build --release
```

### Build Scripts
Located in `scripts/`, these provide more advanced options:
- **Build with UPX compression:** `./scripts/build-release.sh --upx`
- **Fully static Linux build (musl):** `./scripts/build-release.sh --musl`
- **Static build via Docker:** `./scripts/build-release.sh --static`

### Execution
```bash
# Compress a file (overwrites by default)
cargo run -- compress input.tif

# Specific format and level
cargo run -- compress input.tif --output optimized.tif --format zstd --level 19

# Extreme optimization (benchmarks all formats)
cargo run -- compress input.tif --extreme

# Analyze metadata
cargo run -- analyze input.tif
```

---

## Testing and Quality

### Running Tests
```bash
# Run all tests (unit and integration)
cargo test

# Run specific integration tests
cargo test --test integration_tests
```

### Test Reporting
The project includes a robust reporting system to verify compression results and visual integrity.
```bash
# Generate a Markdown test report (processes local images in tests/images/)
./tests/generate-report.sh

# Python-based reporting for more control
python3 tests/generate_test_report.py -i tests/images -o tests/report
```

### Pre-commit Hooks
Enforced via [prek](https://github.com/j178/prek).
- **Setup:** `prek install`
- **Manual Run:** `prek run --all-files`
- **Included Hooks:** `cargo check`, `cargo clippy` (warnings as errors), `cargo fmt`, `cargo test`, `ruff` (for Python scripts).

---

## Development Conventions

- **Code Style:** Strictly follow `cargo fmt` and `cargo clippy`.
- **Error Handling:** Use `anyhow::Result` for application-level errors.
- **FFI Safety:** Wrap `libtiff` calls in `unsafe` blocks and provide safe abstractions where possible.
- **Metadata Preservation:** When adding support for new TIFF tags, update `src/metadata.rs` and ensure they are registered correctly in `src/ffi.rs`.
- **Parallelism:** Maintain file-level parallelism using `rayon`. Avoid complex intra-file parallelism unless dealing with extremely large (multiple GB) files.
- **Dependency Management:** Prefer vendoring C dependencies (like `libtiff`) to ensure a single-binary, zero-dependency output.

### Key Files for Reference
- `Cargo.toml`: Dependency management and build features.
- `src/main.rs`: Subcommand logic and orchestration.
- `src/ffi.rs`: Libtiff API bindings and constants.
- `src/metadata.rs`: Metadata cloning implementation.
- `tests/integration_tests.rs`: Comprehensive test cases for various TIFF formats.
- `prek.toml`: Hook configurations.
