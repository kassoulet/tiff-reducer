## 🤖 Agent Prompt: The TiffThin-RS Architect

**Role:** You are a Senior Systems Engineer specializing in Rust and C-interoperability (FFI).

**Task:** Create a CLI tool in Rust named `tiffthin-rs` that optimizes TIFF files by re-encoding them with high-efficiency codecs (Zstd/LZMA) while strictly preserving all metadata.

**Core Requirements:**

1. **Direct `libtiff` Integration:** Instead of pure-Rust crates, use FFI to interface with the system's `libtiff` (or provide a `build.rs` to link it statically). This is required to support **Predictor 3 (Floating Point)** and **LZMA** which are not fully supported in pure Rust encoders.
2. **The "Tag Pipe" (Metadata Integrity):** * Implement a mechanism to iterate through all tags of the source TIFF.
* Explicitly preserve GeoTIFF keys (Tags `33550, 33922, 34735, 34736, 34737`) and GDAL-specific tags.
* Ensure `BigTIFF` support is toggled if the output file exceeds 4GB.


3. **Quantization Engine (`--quantize`):** * Implement a module to convert `float32` and `int16` images to `uint8`.
* Use a Min-Max scaling algorithm: $pixel_{new} = \frac{pixel_{old} - min}{max - min} \times 255$.


4. **Compression Tournament:** * Implement a "lossless" mode (Zstd + Predictor) and a "lossy" mode (JPEG/WebP).
* Use `rayon` to run compression benchmarks in parallel to find the smallest resulting file size.


5. **Single Binary Packaging:**
* Provide a `Cargo.toml` and `Dockerfile` (using `musl` target) to produce a single, zero-dependency static binary.



**Project Structure:**

* `src/main.rs`: CLI handling with `clap` and progress bars via `indicatif`.
* `src/ffi.rs`: Low-level `extern "C"` bindings for `libtiff`.
* `src/metadata.rs`: Logic for cloning Image File Directories (IFDs).
* `src/quantize.rs`: Logic for 8-bit downsampling.
* `Dockerfile`: Multi-stage build for a scratch-based or alpine-based deployment.

**Constraint:** The app must be able to handle "Deep" TIFFs (32-bit float) and preserve their coordinate systems perfectly.

