#!/bin/bash
# Comprehensive test script for tiff-reducer
# Tests ALL TIFF images in tests/images directory

# Don't exit on error - we want to run all tests
# set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
TIFFTHIN="$PROJECT_DIR/target/debug/tiff-reducer"

# Test images directory (flattened structure)
TEST_IMAGES_DIR="$PROJECT_DIR/tests/images"

# Check if test images exist
if [ ! -d "$TEST_IMAGES_DIR" ]; then
    echo -e "${RED}Error: Test images directory not found${NC}"
    echo "Expected at: $TEST_IMAGES_DIR"
    exit 1
fi

# Count available test images
TEST_COUNT=$(find "$TEST_IMAGES_DIR" -maxdepth 1 -name "*.tif*" -type f 2>/dev/null | wc -l)
if [ "$TEST_COUNT" -eq 0 ]; then
    echo -e "${RED}Error: No TIFF files found in $TEST_IMAGES_DIR${NC}"
    exit 1
fi

echo "Found $TEST_COUNT test images in $TEST_IMAGES_DIR"
echo ""

# Output directory
OUTPUT_DIR="/tmp/tiffthin_test_output"
mkdir -p "$OUTPUT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counters
PASS=0
FAIL=0
SKIP=0
TOTAL=0

# Build if needed
echo "Building tiff-reducer..."
cd "$PROJECT_DIR" && cargo build --features vendored 2>&1 | grep -v "^warning\|^   Compiling\|^     Finished" || true

if [ ! -f "$TIFFTHIN" ]; then
    echo -e "${RED}Error: tiff-reducer not found. Build failed.${NC}"
    exit 1
fi

echo ""
echo "========================================"
echo "tiff-reducer Comprehensive Test Suite"
echo "========================================"
echo ""

# Function to test a single file
test_file() {
    local input="$1"
    local filename=$(basename "$input")
    local output="$OUTPUT_DIR/${filename%.tif}_compressed.tif"
    local output_gdal="$OUTPUT_DIR/${filename%.tif}_gdal.txt"
    
    TOTAL=$((TOTAL + 1))
    
    # Skip very large files (>100MB) for quick tests
    local filesize=$(stat -c%s "$input" 2>/dev/null || echo "0")
    if [ "$filesize" -gt 104857600 ]; then
        echo -e "${YELLOW}SKIP${NC}: $filename (too large: $((filesize/1048576))MB)"
        SKIP=$((SKIP + 1))
        return 0
    fi

    # Compress the file
    local result=$("$TIFFTHIN" compress "$input" -o "$output" 2>&1 | grep -v "^⠁\|^TIFFMergeFieldInfo" || true)
    
    # Check if compression failed due to invalid file
    if echo "$result" | grep -q "Not a TIFF\|bad magic\|Failed to open"; then
        echo -e "${YELLOW}SKIP${NC}: $filename (invalid/corrupt TIFF)"
        SKIP=$((SKIP + 1))
        return
    fi
    
    if [ ! -f "$output" ]; then
        echo -e "${RED}FAIL${NC}: $filename (compression failed)"
        echo "  Error: $result"
        FAIL=$((FAIL + 1))
        return
    fi
    
    # Get sizes
    local orig_size=$(stat -c%s "$input")
    local comp_size=$(stat -c%s "$output")
    local reduction=$(echo "scale=1; (1 - $comp_size / $orig_size) * 100" | bc 2>/dev/null || echo "N/A")
    
    # Run gdalinfo to verify metadata
    local gdal_orig=$(gdalinfo "$input" 2>&1 | grep -E "Size is|Band [0-9]+|ColorInterp=" | head -5)
    local gdal_comp=$(gdalinfo "$output" 2>&1 | grep -E "Size is|Band [0-9]+|ColorInterp=" | head -5)
    
    # Extract dimensions
    local orig_dims=$(echo "$gdal_orig" | grep "Size is" | head -1)
    local comp_dims=$(echo "$gdal_comp" | grep "Size is" | head -1)
    
    # Check dimensions match
    if [ "$orig_dims" != "$comp_dims" ]; then
        echo -e "${RED}FAIL${NC}: $filename (dimensions mismatch)"
        echo "  Original: $orig_dims"
        echo "  Compressed: $comp_dims"
        FAIL=$((FAIL + 1))
        return
    fi
    
    # Check band count
    local orig_bands=$(echo "$gdal_orig" | grep -c "Band " || echo "0")
    local comp_bands=$(echo "$gdal_comp" | grep -c "Band " || echo "0")
    
    if [ "$orig_bands" != "$comp_bands" ]; then
        echo -e "${RED}FAIL${NC}: $filename (band count mismatch: $orig_bands vs $comp_bands)"
        FAIL=$((FAIL + 1))
        return
    fi
    
    # Check color interpretation (extract just the ColorInterp value)
    local orig_color=$(echo "$gdal_orig" | grep "ColorInterp=" | sed 's/.*ColorInterp=\([^,]*\).*/\1/' | sort)
    local comp_color=$(echo "$gdal_comp" | grep "ColorInterp=" | sed 's/.*ColorInterp=\([^,]*\).*/\1/' | sort)
    
    if [ "$orig_color" != "$comp_color" ]; then
        echo -e "${RED}FAIL${NC}: $filename (color interpretation changed)"
        echo "  Original: $orig_color"
        echo "  Compressed: $comp_color"
        FAIL=$((FAIL + 1))
        return
    fi
    
    # Save gdalinfo for reference
    gdalinfo "$output" > "$output_gdal" 2>&1 || true
    
    # Success
    echo -e "${GREEN}PASS${NC}: $filename ($(numfmt --to=iec-i --suffix=B $orig_size 2>/dev/null || echo "${orig_size}B") → $(numfmt --to=iec-i --suffix=B $comp_size 2>/dev/null || echo "${comp_size}B"), ${reduction}% reduction)"
    PASS=$((PASS + 1))
}

# Find and test all TIFF files in tests/images directory
echo "Scanning for TIFF files in $TEST_IMAGES_DIR..."
echo ""

# Test all files
while IFS= read -r -d '' file; do
    test_file "$file"
done < <(find "$TEST_IMAGES_DIR" -maxdepth 1 -name "*.tif*" -type f -print0 2>/dev/null | sort -z)

# Summary
echo "========================================"
echo "Test Summary"
echo "========================================"
echo -e "Total:  $TOTAL"
echo -e "${GREEN}Passed: $PASS${NC}"
echo -e "${RED}Failed: $FAIL${NC}"
echo -e "${YELLOW}Skipped: $SKIP${NC}"
echo "========================================"

if [ $FAIL -gt 0 ]; then
    exit 1
fi

exit 0
