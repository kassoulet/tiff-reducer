#!/bin/bash
# Build release binary with optional UPX compression
#
# Usage:
#   ./scripts/build-release.sh              # Build with vendored features
#   ./scripts/build-release.sh --upx        # Build and compress with UPX
#   ./scripts/build-release.sh --static     # Build fully static via Docker
#   ./scripts/build-release.sh --static --upx  # Build static and compress

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$PROJECT_DIR/target/release"
BINARY_NAME="tiff-reducer"
BINARY_PATH="$OUTPUT_DIR/$BINARY_NAME"

USE_UPX=false
USE_STATIC=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --upx)
            USE_UPX=true
            shift
            ;;
        --static)
            USE_STATIC=true
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --upx      Compress binary with UPX after building"
            echo "  --static   Build fully static binary via Docker"
            echo "  --help     Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Function to check if UPX is available
check_upx() {
    if ! command -v upx &> /dev/null; then
        echo "Error: UPX is not installed"
        echo "Install UPX:"
        echo "  Debian/Ubuntu: sudo apt-get install upx-ucl"
        echo "  Arch Linux: sudo pacman -S upx"
        echo "  macOS: brew install upx"
        exit 1
    fi
}

# Function to build with vendored features
build_vendored() {
    echo "Building with vendored features..."
    cd "$PROJECT_DIR"
    cargo build --release --features vendored
    
    if [ -f "$BINARY_PATH" ]; then
        local size_before=$(stat -c%s "$BINARY_PATH" 2>/dev/null || stat -f%z "$BINARY_PATH" 2>/dev/null)
        echo "Build successful: $BINARY_PATH ($(numfmt --to=iec-i --suffix=B $size_before 2>/dev/null || echo "${size_before} bytes"))"
    else
        echo "Error: Binary not found at $BINARY_PATH"
        exit 1
    fi
}

# Function to build static binary via Docker
build_static() {
    echo "Building fully static binary via Docker..."
    cd "$PROJECT_DIR"
    
    docker build -t tiff-reducer-static .
    
    # Create container and extract binary
    docker create --name temp-tiff-reducer tiff-reducer-static
    docker cp temp-tiff-reducer:/usr/local/bin/tiff-reducer "$BINARY_PATH"
    docker rm temp-tiff-reducer
    
    if [ -f "$BINARY_PATH" ]; then
        local size_before=$(stat -c%s "$BINARY_PATH" 2>/dev/null || stat -f%z "$BINARY_PATH" 2>/dev/null)
        echo "Build successful: $BINARY_PATH ($(numfmt --to=iec-i --suffix=B $size_before 2>/dev/null || echo "${size_before} bytes"))"
    else
        echo "Error: Binary not found at $BINARY_PATH"
        exit 1
    fi
}

# Function to compress with UPX
compress_upx() {
    echo "Compressing with UPX..."
    
    local size_before=$(stat -c%s "$BINARY_PATH" 2>/dev/null || stat -f%z "$BINARY_PATH" 2>/dev/null)
    
    # Compress with maximum compression, preserving executable
    upx --best --lzma "$BINARY_PATH"
    
    if [ -f "$BINARY_PATH" ]; then
        local size_after=$(stat -c%s "$BINARY_PATH" 2>/dev/null || stat -f%z "$BINARY_PATH" 2>/dev/null)
        local reduction=$(echo "scale=1; (1 - $size_after / $size_before) * 100" | bc)
        echo "UPX compression successful!"
        echo "  Before: $(numfmt --to=iec-i --suffix=B $size_before 2>/dev/null || echo "${size_before} bytes")"
        echo "  After:  $(numfmt --to=iec-i --suffix=B $size_after 2>/dev/null || echo "${size_after} bytes")"
        echo "  Reduction: ${reduction}%"
    fi
}

# Main build process
if [ "$USE_STATIC" = true ]; then
    build_static
else
    build_vendored
fi

if [ "$USE_UPX" = true ]; then
    check_upx
    compress_upx
fi

echo ""
echo "Build complete: $BINARY_PATH"
