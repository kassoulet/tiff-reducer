#!/bin/bash
# Script to download test images for tiffthin-rs
# Run this after cloning the repository to get test images

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_IMAGES_DIR="$SCRIPT_DIR/tests/images"

echo "Setting up test images for tiffthin-rs..."
echo ""

mkdir -p "$TEST_IMAGES_DIR"
cd "$TEST_IMAGES_DIR"

# Function to clone with fallback to ZIP download
clone_or_download() {
    local repo_url="$1"
    local repo_name="$2"
    
    echo "Downloading $repo_name..."
    
    # Try git clone first
    if git clone --depth 1 "$repo_url" "$repo_name" 2>/dev/null; then
        echo "  ✓ Cloned successfully"
        return 0
    fi
    
    # Fallback to ZIP download
    echo "  Git clone failed, trying ZIP download..."
    local zip_url="${repo_url%.git}/archive/refs/heads/main.zip"
    
    if command -v curl &> /dev/null; then
        curl -sL "$zip_url" -o "$repo_name.zip"
        unzip -q "$repo_name.zip"
        mv "${repo_name}-main" "$repo_name"
        rm "$repo_name.zip"
        echo "  ✓ Downloaded ZIP successfully"
        return 0
    elif command -v wget &> /dev/null; then
        wget -q "$zip_url" -O "$repo_name.zip"
        unzip -q "$repo_name.zip"
        mv "${repo_name}-main" "$repo_name"
        rm "$repo_name.zip"
        echo "  ✓ Downloaded ZIP successfully"
        return 0
    else
        echo "  ✗ Failed: Neither git, curl, nor wget available"
        return 1
    fi
}

# Download test image repositories
echo "=== Downloading Test Images ==="
echo ""

# Option 1: exampletiffs (smallest, recommended)
if [ ! -d "exampletiffs" ]; then
    clone_or_download "https://github.com/jeremy-lao/exampletiffs.git" "exampletiffs" || true
fi

# Option 2: libtiff-pics
if [ ! -d "libtiff-pics" ]; then
    clone_or_download "https://github.com/ImageMagick/libtiff-pics.git" "libtiff-pics" || true
fi

# Option 3: image-tiff (test images in subdirectory)
if [ ! -d "image-tiff" ]; then
    clone_or_download "https://github.com/image-rs/image-tiff.git" "image-tiff" || true
fi

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Test images location: $TEST_IMAGES_DIR"
echo ""
echo "To run tests:"
echo "  cd $SCRIPT_DIR"
echo "  bash tests/run_all_tests.sh"
echo ""
echo "To run visual regression tests:"
echo "  bash tests/test_visual_regression.sh"
echo ""
echo "To run fuzz tests:"
echo "  bash tests/fuzz_test.sh"
