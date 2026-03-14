# tiff-reducer 🐘

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
- **Rust Integration Tests**: 11/11 passing (compression, metadata, error handling)
- **Visual Regression**: GDAL-based pixel statistics comparison
- **Fuzz Testing**: 18 malformed file scenarios for error handling
- **Benchmark Mode**: `--benchmark` flag for timing/throughput metrics
- **Dry-run Mode**: `--dry-run` flag for benchmarking without writing

## Test Results

### Integration Tests (v0.3.0)
- **Total**: 11 passed, 0 failed
- **Categories**:
  - Compression format tests (Zstd, Deflate, LZW)
  - Metadata preservation (GeoTIFF, ICC profiles, alpha channels)
  - Multi-page TIFF handling
  - Error handling (corrupt files, nonexistent files)
  - Benchmark output validation

### Image Compression
- **Success rate**: ~54% (164/304 images)
- **Working**: Standard bit depths (8/16/32), single-page, strip-based images
- **Known issues**: Multi-page OME-TIFF, some tiled images, non-standard bit depths

### Legacy Tests (v0.2.0)
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
docker build -t tiff-reducer-builder .
# Extract the binary
docker create --name temp-tiff-reducer tiff-reducer-builder
docker cp temp-tiff-reducer:/tiff-reducer ./tiff-reducer
docker rm temp-tiff-reducer
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

Test images are included locally in `tests/images/` directory (304 TIFF files).

To run tests:
```bash
# Run Rust integration tests (recommended)
cargo test --test integration_tests handling

# Generate HTML Visual Test Report (easy way)
./tests/generate-report.sh

# Generate HTML Visual Test Report (manual)
python3 tests/generate_html_report.py -i tests/images -o tests/report -n 20

# View report in browser
./tests/generate-report.sh --open    # Auto-open after generation
xdg-open tests/report/index.html     # Linux (manual)
open tests/report/index.html         # macOS (manual)
```

**Script Options:**
```bash
./tests/generate-report.sh -n 50           # Process 50 images (default: 20)
./tests/generate-report.sh -f deflate -l 9 # Use Deflate compression
./tests/generate-report.sh --open          # Open in browser after generation
./tests/generate-report.sh --help          # Show all options
```

**HTML Report Features:**
- Side-by-side image comparison (thumbnails)
- Metadata comparison tables (dimensions, bands, compression, resolution)
- Quality metrics (PSNR, SSIM)
- Pass/fail indicators with color coding
- Summary dashboard with statistics

**Note:** Test images include various formats:
- Standard TIFF files (RGB, grayscale, palette)
- Multi-page TIFF files
- OME-TIFF files (microscopy data)
- GeoTIFF files (geospatial data)
- Various compression formats (LZW, Deflate, JPEG, etc.)

## Usage

### Compress a file (Overwrites by default)
```bash
tiff-reducer compress image.tif
```

### With specific format and level
```bash
tiff-reducer compress input.tif --output optimized.tif --format zstd --level 22
```

### Extreme Optimization with Quantization
```bash
tiff-reducer compress input.tif --output optimized.tif --extreme --quantize
```

### Benchmark Mode (timing and throughput)
```bash
tiff-reducer compress input.tif --output optimized.tif --benchmark
```

### Dry-run Mode (benchmark without writing)
```bash
tiff-reducer compress input.tif --dry-run --benchmark
```

### Control Parallelism (default: number of CPUs)
```bash
tiff-reducer compress ./input_folder --output ./output_folder --jobs 4
```

### LERC Compression (for scientific data)
```bash
tiff-reducer compress input.tif --output optimized.tif --format lerc
tiff-reducer compress input.tif --output optimized.tif --format lerc-zstd
```

### Analyze Metadata
```bash
tiff-reducer analyze image.tif
```

### Process a directory
```bash
tiff-reducer compress ./input_folder --output ./output_folder --extreme
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

## Test Images

Test images are sourced from:
- [exampletiffs](https://github.com/jeremy-lao/exampletiffs.git)
- [libtiff-pics](https://github.com/ImageMagick/libtiff-pics.git)
- [image-tiff](https://github.com/image-rs/image-tiff.git)

See [CREDITS.md](CREDITS.md) for details.

## Author
Gautier Portet <kassoulet@gmail.com>
