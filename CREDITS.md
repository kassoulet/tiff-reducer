# Test Images Credits

The test images in `tests/images/` directory are collected from the following open-source repositories:

## Primary Sources

### 1. exampletiffs
- **Repository**: https://github.com/jeremy-lao/exampletiffs.git
- **License**: Not specified (assumed open for testing purposes)
- **Content**: Various TIFF format examples including multi-page, OME-TIFF, and specialized formats

### 2. libtiff-pics
- **Repository**: https://github.com/ImageMagick/libtiff-pics.git
- **License**: Likely part of ImageMagick project (Apache 2.0)
- **Content**: Standard TIFF test images used by ImageMagick for testing

### 3. image-tiff
- **Repository**: https://github.com/image-rs/image-tiff.git
- **License**: MIT/Apache 2.0 (image-rs project)
- **Content**: TIFF test images for the Rust image-tiff library

## Test Image Categories

The combined test suite (304 files) includes:

- **Standard TIFF**: RGB, grayscale, palette images
- **Multi-page TIFF**: Sequential images, time series
- **OME-TIFF**: Microscopy data (Open Microscopy Environment)
- **GeoTIFF**: Geospatial data with metadata
- **Various bit depths**: 1-bit, 2-bit, 4-bit, 8-bit, 12-bit, 16-bit, 32-bit
- **Various compression**: Uncompressed, LZW, Deflate, JPEG, PackBits
- **Color spaces**: RGB, CMYK, YCbCr, palette
- **Special formats**: BigTIFF, tiled, planar configuration

## Usage

These test images are used for:
- Integration testing of tiff-reducer
- Visual regression testing
- Fuzz testing with malformed files
- Benchmarking compression performance

## Attribution

If you use these test images in your own projects, please refer to the original repositories for licensing terms.
