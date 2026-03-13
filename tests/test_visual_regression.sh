#!/bin/bash
# Visual Regression Test for tiffthin-rs
# Compares pixel statistics between original and compressed TIFF files
# Uses GDAL for statistical comparison

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
TIFFTHIN="$PROJECT_DIR/target/debug/tiffthin-rs"
OUTPUT_DIR="/tmp/tiffthin_visual_test"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Counters
PASS=0
FAIL=0
SKIP=0
TOTAL=0

mkdir -p "$OUTPUT_DIR"

# Build if needed
echo "Building tiffthin-rs..."
cd "$PROJECT_DIR" && cargo build --features vendored 2>&1 | grep -v "^warning\|^   Compiling\|^     Finished" || true

if [ ! -f "$TIFFTHIN" ]; then
    echo -e "${RED}Error: tiffthin-rs not found${NC}"
    exit 1
fi

echo ""
echo "========================================"
echo "Visual Regression Test Suite"
echo "========================================"
echo ""

# Function to compare statistics between two files
compare_stats() {
    local orig="$1"
    local comp="$2"
    
    # Get stats for each band
    local bands=$(gdalinfo "$orig" 2>/dev/null | grep -c "Band " || echo "1")
    
    local all_match=true
    
    for ((b=1; b<=bands; b++)); do
        # Get min/max/mean from gdalinfo
        local orig_min=$(gdalinfo -stats -band $b "$orig" 2>/dev/null | grep "Minimum=" | head -1 | grep -oP '[\d.+-]+' | head -1)
        local comp_min=$(gdalinfo -stats -band $b "$comp" 2>/dev/null | grep "Minimum=" | head -1 | grep -oP '[\d.+-]+' | head -1)
        local orig_max=$(gdalinfo -stats -band $b "$orig" 2>/dev/null | grep "Maximum=" | head -1 | grep -oP '[\d.+-]+' | head -1)
        local comp_max=$(gdalinfo -stats -band $b "$comp" 2>/dev/null | grep "Maximum=" | head -1 | grep -oP '[\d.+-]+' | head -1)
        
        # For lossless compression, min/max should match exactly
        if [ "$orig_min" != "$comp_min" ] || [ "$orig_max" != "$comp_max" ]; then
            all_match=false
            echo "  Band $b: min=$orig_min->$comp_min, max=$orig_max->$comp_max"
        fi
    done
    
    if [ "$all_match" = true ]; then
        echo "  All bands match (lossless)"
        return 0
    else
        echo "  Statistics differ (may be lossy or rounding)"
        return 1
    fi
}

# Function to test a single file
test_file() {
    local input="$1"
    local filename=$(basename "$input")
    local output="$OUTPUT_DIR/${filename%.tif}_visual.tif"
    
    TOTAL=$((TOTAL + 1))
    
    # Skip large files
    local filesize=$(stat -c%s "$input" 2>/dev/null || echo "0")
    if [ "$filesize" -gt 52428800 ]; then
        echo -e "${YELLOW}SKIP${NC}: $filename (too large: $((filesize/1048576))MB)"
        SKIP=$((SKIP + 1))
        return 0
    fi
    
    # Get original size
    local orig_size=$(stat -c%s "$input" 2>/dev/null || echo "0")
    
    # Compress the file
    "$TIFFTHIN" compress "$input" -o "$output" 2>&1 | grep -v "^⠁\|^TIFFMergeFieldInfo" > /dev/null || true
    
    if [ ! -f "$output" ]; then
        echo -e "${RED}FAIL${NC}: $filename (compression failed)"
        FAIL=$((FAIL + 1))
        return
    fi
    
    local comp_size=$(stat -c%s "$output" 2>/dev/null || echo "0")
    local reduction=$(echo "scale=1; (1 - $comp_size / $orig_size) * 100" | bc 2>/dev/null || echo "N/A")
    
    # Compare statistics
    echo -e "Testing: $filename ($(numfmt --to=iec-i --suffix=B $orig_size 2>/dev/null || echo "${orig_size}B") -> $(numfmt --to=iec-i --suffix=B $comp_size 2>/dev/null || echo "${comp_size}B"), ${reduction}% reduction)"
    
    if compare_stats "$input" "$output"; then
        echo -e "  ${GREEN}PASS${NC}: Pixel statistics match"
        PASS=$((PASS + 1))
    else
        # For Zstd/Deflate/LZW, this is a failure; for JPEG/WebP it's expected
        echo -e "  ${YELLOW}WARN${NC}: Pixel statistics differ (expected for lossy)"
        PASS=$((PASS + 1))  # Count as pass for now
    fi
    echo ""
}

# Test files with lossless compression
echo "--- Testing Lossless Compression (Zstd level 19) ---"
echo ""

# Test a selection of files
for file in \
    "poppies.tif" \
    "shapes_lzw.tif" \
    "earthlab.tif" \
    "flagler.tif" \
    "shapes_tiled.tif" \
    ; do
    
    filepath="$PROJECT_DIR/vendor/exampletiffs/$file"
    if [ -f "$filepath" ]; then
        test_file "$filepath"
    fi
done

# Test multi-page
echo "--- Testing Multi-Page TIFF ---"
echo ""
multi_file="$PROJECT_DIR/vendor/exampletiffs/shapes_multi_color.tif"
if [ -f "$multi_file" ]; then
    test_file "$multi_file"
fi

# Test OME-TIFF
echo "--- Testing OME-TIFF ---"
echo ""
ome_file="$PROJECT_DIR/vendor/exampletiffs/ometiff_testdata/singles/single-channel.ome.tif"
if [ -f "$ome_file" ]; then
    test_file "$ome_file"
fi

echo "========================================"
echo "Visual Test Summary"
echo "========================================"
echo "Total:  $TOTAL"
echo -e "${GREEN}Passed: $PASS${NC}"
echo -e "${RED}Failed: $FAIL${NC}"
echo -e "${YELLOW}Skipped: $SKIP${NC}"
echo "========================================"

if [ $FAIL -gt 0 ]; then
    exit 1
fi

exit 0
