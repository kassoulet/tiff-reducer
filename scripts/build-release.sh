#!/bin/bash
# Build release binary with optional UPX compression
#
# Usage:
#   ./scripts/build-release.sh              # Build with vendored features
#   ./scripts/build-release.sh --upx        # Build and compress with UPX
#   ./scripts/build-release.sh --static     # Build fully static via Docker (Linux only)
#   ./scripts/build-release.sh --static --upx  # Build static and compress
#   ./scripts/build-release.sh --musl       # Build with musl target (fully static)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Detect target directory from cargo metadata or use default
TARGET_DIR=$(cargo metadata --format-version 1 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('target_directory'))" 2>/dev/null || echo "$PROJECT_DIR/target")

USE_UPX=false
USE_STATIC=false
USE_MUSL=false

# Determine target triple
TARGET_TRIPLE=""
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    TARGET_TRIPLE="x86_64-unknown-linux-musl"
    OUTPUT_DIR="$TARGET_DIR/$TARGET_TRIPLE/release"
elif [[ "$OSTYPE" == "darwin"* ]]; then
    TARGET_TRIPLE="x86_64-apple-darwin"
    OUTPUT_DIR="$TARGET_DIR/$TARGET_TRIPLE/release"
else
    OUTPUT_DIR="$TARGET_DIR/release"
fi

BINARY_NAME="tiff-reducer"
BINARY_PATH="$OUTPUT_DIR/$BINARY_NAME"

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
        --musl)
            USE_MUSL=true
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --upx      Compress binary with UPX after building"
            echo "  --static   Build fully static binary via Docker (Linux only)"
            echo "  --musl     Build with musl target for fully static binary"
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
    
    if [ "$USE_MUSL" = true ]; then
        echo "Target: $TARGET_TRIPLE (musl - fully static)"
        cargo build --release --features vendored --target "$TARGET_TRIPLE"
    else
        echo "Target: native"
        cargo build --release --features vendored
    fi

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
    echo "Building fully static binary via Docker (musl + vendored)..."
    cd "$PROJECT_DIR"

    docker build -f Dockerfile.static -t tiff-reducer-static .

    # Remove any existing container with the same name
    docker rm -f temp-tiff-reducer 2>/dev/null || true

    # Create container and extract binary
    docker create --name temp-tiff-reducer tiff-reducer-static
    docker cp temp-tiff-reducer:/tiff-reducer "$BINARY_PATH"
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
elif [ "$USE_MUSL" = true ]; then
    build_vendored
else
    build_vendored
fi

if [ "$USE_UPX" = true ]; then
    check_upx
    compress_upx
fi

echo ""
echo "Build complete: $BINARY_PATH"
