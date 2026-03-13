#!/bin/bash
# Test script to verify tiffthin-rs preserves metadata while compressing

set -e

TEST_DIR="/tmp/tiff_test"
OUTPUT_DIR="/tmp/tiff_test_output"
TIFFTHIN="./target/debug/tiffthin-rs"

mkdir -p "$OUTPUT_DIR"

echo "=== tiffthin-rs GDAL Metadata Preservation Test ==="
echo ""

# Test files
TEST_FILES=(
    "poppies.tif"
    "bali.tif"
    "shapes_lzw.tif"
    "shapes_tiled_multi.tif"
)

PASS=0
FAIL=0

for file in "${TEST_FILES[@]}"; do
    INPUT="$TEST_DIR/$file"
    OUTPUT="$OUTPUT_DIR/${file%.tif}_compressed.tif"
    
    if [ ! -f "$INPUT" ]; then
        echo "SKIP: $file not found"
        continue
    fi
    
    echo "----------------------------------------"
    echo "Testing: $file"
    echo "----------------------------------------"
    
    # Get original size
    ORIG_SIZE=$(stat -c%s "$INPUT")
    
    # Get original gdalinfo (excluding size-related fields)
    echo "Running gdalinfo on original..."
    gdalinfo "$INPUT" > "$OUTPUT_DIR/original_${file%.tif}.txt" 2>&1 || true
    
    # Compress with tiffthin-rs
    echo "Compressing with tiffthin-rs..."
    $TIFFTHIN compress "$INPUT" -o "$OUTPUT" 2>&1 | grep -v "^⠁\|^TIFFMergeFieldInfo" || true
    
    # Get compressed size
    COMP_SIZE=$(stat -c%s "$OUTPUT")
    
    # Get compressed gdalinfo
    echo "Running gdalinfo on compressed..."
    gdalinfo "$OUTPUT" > "$OUTPUT_DIR/compressed_${file%.tif}.txt" 2>&1 || true
    
    # Extract key metadata for comparison (excluding compression and file size)
    # Compare: dimensions, coordinate system, geotransform, bands, data type
    
    echo ""
    echo "Original size:  $ORIG_SIZE bytes"
    echo "Compressed size: $COMP_SIZE bytes"
    
    # Calculate reduction
    if [ $ORIG_SIZE -gt 0 ]; then
        REDUCTION=$(echo "scale=1; (1 - $COMP_SIZE / $ORIG_SIZE) * 100" | bc)
        echo "Reduction: ${REDUCTION}%"
    fi
    
    # Extract and compare critical metadata
    echo ""
    echo "Comparing metadata..."
    
    # Extract dimensions
    ORIG_DIMS=$(grep -E "Size is [0-9]+," "$OUTPUT_DIR/original_${file%.tif}.txt" | head -1)
    COMP_DIMS=$(grep -E "Size is [0-9]+," "$OUTPUT_DIR/compressed_${file%.tif}.txt" | head -1)
    
    if [ "$ORIG_DIMS" = "$COMP_DIMS" ]; then
        echo "✓ Dimensions match: $ORIG_DIMS"
    else
        echo "✗ Dimensions MISMATCH!"
        echo "  Original:    $ORIG_DIMS"
        echo "  Compressed:  $COMP_DIMS"
        FAIL=$((FAIL + 1))
        continue
    fi
    
    # Extract band count and type
    ORIG_BANDS=$(grep -E "Band [0-9]+ Block" "$OUTPUT_DIR/original_${file%.tif}.txt" | head -1)
    COMP_BANDS=$(grep -E "Band [0-9]+ Block" "$OUTPUT_DIR/compressed_${file%.tif}.txt" | head -1)
    
    # Compare band count (extract just the band number pattern)
    ORIG_BAND_COUNT=$(echo "$ORIG_BANDS" | grep -oP "Band \K[0-9]+" | head -1)
    COMP_BAND_COUNT=$(echo "$COMP_BANDS" | grep -oP "Band \K[0-9]+" | head -1)
    
    if [ "$ORIG_BAND_COUNT" = "$COMP_BAND_COUNT" ]; then
        echo "✓ Band count matches: $ORIG_BAND_COUNT"
    else
        echo "✗ Band count MISMATCH!"
        echo "  Original:    $ORIG_BAND_COUNT"
        echo "  Compressed:  $COMP_BAND_COUNT"
        FAIL=$((FAIL + 1))
        continue
    fi
    
    # Check for GeoTIFF tags preservation (if present)
    if grep -q "Origin = " "$OUTPUT_DIR/original_${file%.tif}.txt"; then
        ORIG_ORIGIN=$(grep "Origin = " "$OUTPUT_DIR/original_${file%.tif}.txt")
        COMP_ORIGIN=$(grep "Origin = " "$OUTPUT_DIR/compressed_${file%.tif}.txt")
        if [ "$ORIG_ORIGIN" = "$COMP_ORIGIN" ]; then
            echo "✓ Geo origin preserved"
        else
            echo "✗ Geo origin MISMATCH!"
            FAIL=$((FAIL + 1))
        fi
    fi
    
    if grep -q "Pixel Size = " "$OUTPUT_DIR/original_${file%.tif}.txt"; then
        ORIG_PIXEL=$(grep "Pixel Size = " "$OUTPUT_DIR/original_${file%.tif}.txt")
        COMP_PIXEL=$(grep "Pixel Size = " "$OUTPUT_DIR/compressed_${file%.tif}.txt")
        if [ "$ORIG_PIXEL" = "$COMP_PIXEL" ]; then
            echo "✓ Pixel size preserved"
        else
            echo "✗ Pixel size MISMATCH!"
            FAIL=$((FAIL + 1))
        fi
    fi
    
    # Check compression changed
    ORIG_COMP=$(grep "Compression=" "$OUTPUT_DIR/original_${file%.tif}.txt" | head -1 || echo "none")
    COMP_COMP=$(grep "Compression=" "$OUTPUT_DIR/compressed_${file%.tif}.txt" | head -1 || echo "none")
    
    echo ""
    echo "Compression: $ORIG_COMP -> $COMP_COMP"
    
    PASS=$((PASS + 1))
    echo ""
    echo "RESULT: PASS"
done

echo ""
echo "========================================"
echo "Summary: $PASS passed, $FAIL failed"
echo "========================================"

if [ $FAIL -gt 0 ]; then
    exit 1
fi
