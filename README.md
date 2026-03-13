# TiffThin-RS 🐘

A high-performance Rust CLI tool for optimizing TIFF files using high-efficiency codecs (Zstd/LZMA/LERC) while strictly preserving all metadata (GeoTIFF, ICC, OME-XML, etc.).

## Features

### Compression
- **Multiple Codecs**: Zstd, LZMA, Deflate, LZW, JPEG, WebP, LERC (with variants)
- **Compression Levels**: Zstd (1-22), Deflate/LZMA (1-9), JPEG/WebP (1-100)
- **Predictors**: None, Horizontal, Floating Point (for float32 data)
- **SIMD Optimizations**: SSE4.2/AVX2 (x86_64) or NEON (ARM64) for ~12% speedup
- **Extreme Mode**: Benchmarks all formats to find the smallest file size

### Metadata Preservation
- **GeoTIFF**: Tags 33550, 33922, 34735, 34736, 34737 (ModelPixelScale, ModelTiepoint, GeoKeyDirectory, etc.)
- **ICC Profiles**: Full color profile preservation (tag 34675)
- **Alpha Channels**: ExtraSamples tag (#338) for proper alpha interpretation
- **YCbCr**: Subsampling, positioning, and conversion coefficients
- **CMYK**: InkSet, DotRange, InkNames, NumberOfInks
- **OME-XML**: ImageDescription tag for microscopy data
- **Colormap**: Palette/colormap preservation for indexed images

### File Support
- **Multi-page TIFF**: Iterates through all IFDs (pages)
- **Tiled TIFF**: Auto-detection and tiled processing
- **BigTIFF**: Automatic handling of files >4GB
- **Quantization**: Convert float32/int16 to uint8

### Testing & Quality
- **Visual Regression**: GDAL-based pixel statistics comparison
- **Fuzz Testing**: 18 malformed file scenarios for error handling
- **Benchmark Mode**: `--benchmark` flag for timing/throughput metrics

## Test Results
- **Metadata tests**: 27 passed, 0 failed, 29 skipped (56 files)
- **Visual tests**: 6/6 passed (lossless pixel-perfect compression)
- **Fuzz tests**: 16/18 passed (graceful error handling)

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

### Development Build (with Test Images)

This project uses git submodules for test images. To clone with test images:

```bash
# Clone with submodules
git clone --recurse-submodules https://github.com/youruser/tiffthin-rs.git

# Or initialize submodules after cloning
git clone https://github.com/youruser/tiffthin-rs.git
cd tiffthin-rs
git submodule update --init --recursive
```

**Test images location:** `tests/images/`
- `tests/images/exampletiffs/` - Test images from exampletiffs repository
- `tests/images/libtiff-pics/` - Test images from libtiff-pics repository
- `tests/images/image-tiff/` - Test images from image-tiff crate repository

**Manual setup (if submodules fail):**
```bash
# Create test images directory
mkdir -p tests/images

# Download test images manually (choose one or more)
cd tests/images

# Option 1: exampletiffs
git clone https://github.com/jeremy-lao/exampletiffs.git

# Option 2: libtiff-pics  
git clone https://github.com/ImageMagick/libtiff-pics.git

# Option 3: image-tiff test images
git clone https://github.com/image-rs/image-tiff.git
# Test images are in: image-tiff/test_images/

cd ../..
```

**Note:** If you encounter authentication issues with GitHub, you may need to:
1. Set up SSH keys: `ssh-keygen` and add to GitHub
2. Or use HTTPS with a personal access token
3. Or download repositories as ZIP files and extract

## Usage

### Compress a file (Overwrites by default)
```bash
tiffthin-rs compress image.tif
```

### With specific format and level
```bash
tiffthin-rs compress input.tif --output optimized.tif --format zstd --level 22
```

### Extreme Optimization with Quantization
```bash
tiffthin-rs compress input.tif --output optimized.tif --extreme --quantize
```

### Benchmark Mode (timing and throughput)
```bash
tiffthin-rs compress input.tif --output optimized.tif --benchmark
```

### Control Parallelism (default: number of CPUs)
```bash
tiffthin-rs compress ./input_folder --output ./output_folder --jobs 4
```

### LERC Compression (for scientific data)
```bash
tiffthin-rs compress input.tif --output optimized.tif --format lerc
tiffthin-rs compress input.tif --output optimized.tif --format lerc-zstd
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
- `-f, --format <FORMAT>`: Manually choose format (`zstd`, `lzma`, `lzw`, `deflate`, `jpeg`, `webp`, `lerc`, `lerc-deflate`, `lerc-zstd`, `jpeg-xl`).
- `-l, --level <LEVEL>`: Compression level (Zstd: 1-22, Deflate/LZMA: 1-9, JPEG/WebP: 1-100).
- `--extreme`: Try all formats and pick the winner.
- `--quantize`: Convert to 8-bit uint.
- `--benchmark`: Display timing and throughput metrics.
- `-j, --jobs <JOBS>`: Number of parallel jobs (default: number of CPUs).
- `--dry-run`: Benchmark without writing to disk.

### `analyze`
- Displays dimensions, channels, bit depth, format, and current compression.

## License
MIT

## Author
Gautier Portet <kassoulet@gmail.com>
