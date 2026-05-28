# tiff-reducer 🐘

A high-performance Rust CLI tool for optimizing TIFF files using high-efficiency codecs (Zstd/LZMA/LERC) while strictly preserving all metadata (GeoTIFF, ICC, OME-XML, etc.).

## Features

> One single executable, no dependencies, no configuration files. No need to install any additional software. No need to configure anything.

### Compression
- **Multiple Codecs**: Zstd, LZMA, Deflate, LZW, JPEG, WebP, LERC, **Uncompressed**
- **Compression Levels**: Zstd (1-22), Deflate/LZMA (1-9), JPEG/WebP (1-100)
- **Predictors**: None, Horizontal, Floating Point (for float32 data)
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
- **Rust Integration Tests**: 10/10 passing (compression, metadata, error handling, uncompressed format)
- **Visual Regression**: GDAL-based pixel statistics comparison and Rust reporting
- **Fuzz Testing**: 18 malformed file scenarios for error handling
- **Benchmark Mode**: `--benchmark` flag for timing/throughput metrics
- **Dry-run Mode**: `--dry-run` flag for benchmarking without writing
- **Visual Reports**: Automatic Markdown report generation with thumbnails

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

### Lossy Optimization (WebP + JPEG)
Tries both WebP and JPEG and selects the smallest result. Default quality is 90.
```bash
tiff-reducer compress input.tif --output optimized.tif --lossy --level 85
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

### Decompress to Uncompressed TIFF
```bash
tiff-reducer compress compressed.tif --output decompressed.tif --format uncompressed
```

### Analyze Metadata
```bash
tiff-reducer analyze image.tif
```

### Process a directory
```bash
tiff-reducer compress ./input_folder --output ./output_folder --extreme
```

## Installation

### Build from Source (Vendored - Recommended)

Builds libtiff and all compression libraries from source. No external library dependencies required.

**Prerequisites:**
```bash
# Linux
sudo apt-get install -y cmake git

# macOS
xcode-select --install
brew install cmake

# Windows
# Install Visual Studio Build Tools with C++ support
```

**Build:**
```bash
# Default vendored build (all compression libraries from source)
cargo build --release
```

**Cross-Platform Targets:**

| Platform | Target | Command |
|----------|--------|---------|
| **Linux (fully static)** | `x86_64-unknown-linux-musl` | `cargo build --release --target x86_64-unknown-linux-musl` |
| **Linux (glibc)** | native | `cargo build --release` |
| **macOS (Intel)** | `x86_64-apple-darwin` | `cargo build --release --target x86_64-apple-darwin` |
| **macOS (Apple Silicon)** | `aarch64-apple-darwin` | `cargo build --release --target aarch64-apple-darwin` |
| **Windows** | `x86_64-pc-windows-msvc` | `cargo build --release` |

**Using the build script:**
```bash
# Linux - fully static with UPX compression
./scripts/build-release.sh --musl --upx

# Native build with UPX compression
./scripts/build-release.sh --upx
```

### UPX Compression (Optional - Reduces binary size by ~60-70%)

After building, you can compress the binary with [UPX](https://github.com/upx/upx):

```bash
# Install UPX
# Debian/Ubuntu
sudo apt-get install upx-ucl

# Arch Linux
sudo pacman -S upx

# macOS
brew install upx
```

**Compress the binary:**
```bash
# Build first
cargo build --release

# Then compress
upx --best --lzma ./target/release/tiff-reducer
```

**Using the build script (recommended):**
```bash
# Build with vendored features and compress with UPX
./scripts/build-release.sh --upx

# Build fully static via Docker and compress with UPX
./scripts/build-release.sh --static --upx
```

**Example size reduction:**
```
Before UPX: 4.2 MB
After UPX:  1.3 MB (69% reduction)
```

### Development Build (with Test Images)

Test images are included locally in `tests/images/` directory (304 TIFF files).

To run all tests and generate a report:
```bash
# Run all tests (unit + integration) and generate report
./scripts/run-tests.sh
```

**Runner Options:**
```bash
./scripts/run-tests.sh --help

Options:
  -f, --format FORMAT    Compression format for report (default: zstd)
  -l, --level LEVEL      Compression level for report (default: 19)
  -n, --limit NUM        Limit number of images in report (default: all)
  -o, --output PATH      Output path for report (default: tests/README.md)
```

**Markdown Report Features:**
- Summary statistics (pass/fail counts and percentages)
- Side-by-side thumbnail comparison (Original vs Compressed)
- Performance metrics (throughput, average time)
- List of failed images with error messages

**Note:** Test images include various formats:
- Standard TIFF files (RGB, grayscale, palette)
- Multi-page TIFF files
- OME-TIFF files (microscopy data)
- GeoTIFF files (geospatial data)
- Various compression formats (LZW, Deflate, JPEG, etc.)

## Subcommands & Options

### `compress`
- `-o, --output <PATH>`: Specify output file or directory.
- `-f, --format <FORMAT>`: Manually choose format (`zstd`, `lzma`, `lzw`, `deflate`, `jpeg`, `webp`, `lerc`, `lerc-deflate`, `lerc-zstd`, `jpeg-xl`, `uncompressed`).
- `-l, --level <LEVEL>`: Compression level (Zstd: 1-22, Deflate/LZMA: 1-9, JPEG/WebP: 1-100).
- `--extreme`: Try all formats and pick the winner.
- `--quantize`: Convert to 8-bit uint.
- `--benchmark`: Display timing and throughput metrics.
- `-j, --jobs <JOBS>`: Number of parallel jobs (default: number of CPUs).
- `--dry-run`: Benchmark without writing to disk.

### `analyze`
- Displays dimensions, channels, bit depth, format, and current compression.

## Development

### CI/CD

This project uses GitHub Actions for continuous integration and testing.

**Workflows:**
- **CI** (`.github/workflows/ci.yml`): Build, format check, clippy, error handling tests, and markdown test report
- **Release** (`.github/workflows/release.yml`): Automated release creation with UPX compression

**Test Report:**
- Runs on push to `kassoulet/tiff-reducer` repository
- Processes 20 test images with ZSTD compression
- Uploads markdown report and thumbnails as CI artifacts (7-day retention)
- View artifacts from GitHub Actions run page

### Prek Hooks

This project uses [prek](https://github.com/j178/prek), a high-performance Rust-based pre-commit framework, to enforce code quality standards.

**Setup:**
```bash
# Install uv (if not already installed)
curl -LsSf https://astral.sh/uv/install.sh | sh

# Install prek
uv tool install prek

# Install git hooks
prek install
```

**Hooks configured:**
- `cargo check` - Rust compilation check
- `cargo clippy` - Rust linter (warnings as errors)
- `cargo fmt` - Rust code formatting
- `cargo test` - Rust integration tests (error handling)

**Manual run:**
```bash
prek run --all-files
```

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
