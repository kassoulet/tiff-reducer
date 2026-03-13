# TiffThin-RS 🐘

A high-performance Rust CLI tool for optimizing TIFF files using high-efficiency codecs (Zstd/LZMA) while strictly preserving all metadata (GeoTIFF, GDAL, etc.).

## Features

- **Direct `libtiff` Integration**: Uses FFI to interface with the system's `libtiff`, supporting advanced features like **Predictor 3 (Floating Point)** and **LZMA**.
- **Metadata Integrity (Tag Pipe)**: Iterates through all tags and explicitly preserves GeoTIFF keys and GDAL-specific tags.
- **Quantization Engine**: Convert `float32` and `int16` images to `uint8` using Min-Max scaling.
- **Extreme Mode (Compression Tournament)**: Benchmarks multiple compression formats (Zstd, LZMA, LZW, Deflate) in parallel to find the smallest file size.
- **BigTIFF Support**: Automatically handles files exceeding 4GB.
- **Static Binary**: Produced via a multi-stage Docker build for zero-dependency deployment.

## Installation

### From Source (Dynamic Linking)
Requires `libtiff`, `libzstd`, `liblzma`, and `libjpeg` development headers.

**Debian/Ubuntu:**
```bash
sudo apt-get install -y libtiff-dev libzstd-dev liblzma-dev libjpeg-dev libwebp-dev libdeflate-dev
cargo build --release
```

**Arch Linux:**
```bash
sudo pacman -S libtiff zstd xz libjpeg-turbo libwebp libdeflate
cargo build --release
```

### Static Binary (via Docker - Recommended for portability)
Produces a musl-linked binary for any Linux environment.

```bash
docker build -t tiffthin-builder .
# Extract the binary
docker create --name temp-tiffthin tiffthin-builder
docker cp temp-tiffthin:/tiffthin-rs ./tiffthin-rs
docker rm temp-tiffthin
```

### Vendored Build (Statically linked compression libraries)
Builds all compression libraries from source. Only glibc remains dynamic.

```bash
# No system dependencies needed (except build tools)
sudo apt-get install -y cmake git

# Build with all libraries compiled from source
cargo build --release --features vendored
```

**What's static vs dynamic:**

| Library | Status |
|---------|--------|
| libtiff | ✅ Static (built from source) |
| libdeflate | ✅ Static (built from source) |
| libzstd | ✅ Static (built from source) |
| liblzma | ✅ Static (built from source) |
| libjpeg | ✅ Static (built from source) |
| zlib | ✅ Static (built from source) |
| glibc | ⚠️ Dynamic (system C library) |

**Result:** The binary only depends on glibc (`libc.so.6`, `libm.so.6`, `libgcc_s.so.1`). All compression libraries are statically linked.

For a **fully static binary** (no glibc dependency), use the Docker build or compile with `--target x86_64-unknown-linux-musl` (requires musl toolchain).

## Usage

### Compress a file (Overwrites by default)
```bash
tiffthin-rs compress image.tif
```

### Extreme Optimization with Quantization
```bash
tiffthin-rs compress input.tif --output optimized.tif --extreme --quantize
```

### Analyze Metadata
```bash
tiffthin-rs analyze image.tif
```

### Process a directory
```bash
tiffthin-rs compress ./input_folder --output ./output_folder --extreme
```

## Subcommands & Options

### `compress`
- `-o, --output <PATH>`: Specify output file or directory.
- `-f, --format <FORMAT>`: Manually choose format (`zstd`, `lzma`, `lzw`, `deflate`, `jpeg`, `webp`).
- `--extreme`: Try all formats and pick the winner.
- `--quantize`: Convert to 8-bit uint.
- `--dry-run`: Benchmark without writing to disk.

### `analyze`
- Displays dimensions, channels, bit depth, format, and current compression.

## License
MIT

## Author
Gautier Portet <kassoulet@gmail.com>
