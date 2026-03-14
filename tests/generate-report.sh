#!/bin/bash
#
# Generate HTML Visual Test Report for tiff-reducer
#
# This script builds the project and generates an HTML report with:
# - Side-by-side image comparison (thumbnails)
# - Metadata comparison tables
# - Quality metrics (PSNR, SSIM)
# - Pass/fail indicators with color coding
# - Summary dashboard with statistics
#
# Usage:
#   ./tests/generate-report.sh [options]
#
# Options:
#   -n, --number    Number of test images to process (default: 20)
#   -f, --format    Compression format (default: zstd)
#   -l, --level     Compression level (default: 19)
#   -o, --output    Output directory (default: tests/report)
#   -h, --help      Show this help message
#
# Examples:
#   ./tests/generate-report.sh                    # Generate report for 20 images
#   ./tests/generate-report.sh -n 50              # Generate report for 50 images
#   ./tests/generate-report.sh -f deflate -l 9    # Use Deflate compression
#   ./tests/generate-report.sh --open             # Generate and open in browser
#

set -e

# Default values
NUM_IMAGES=20
FORMAT="zstd"
LEVEL=19
OUTPUT_DIR="tests/report"
OPEN_BROWSER=false
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BINARY_PATH="$PROJECT_ROOT/target/release/tiff-reducer"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print colored message
print_msg() {
    local color=$1
    local msg=$2
    echo -e "${color}${msg}${NC}"
}

# Show help
show_help() {
    head -30 "$0" | tail -25
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -n|--number)
            NUM_IMAGES="$2"
            shift 2
            ;;
        -f|--format)
            FORMAT="$2"
            shift 2
            ;;
        -l|--level)
            LEVEL="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --open)
            OPEN_BROWSER=true
            shift
            ;;
        -h|--help)
            show_help
            ;;
        *)
            print_msg "$RED" "Unknown option: $1"
            echo "Use -h or --help for usage information"
            exit 1
            ;;
    esac
done

# Check prerequisites
check_prerequisites() {
    print_msg "$BLUE" "Checking prerequisites..."

    # Check for Python 3
    if ! command -v python3 &> /dev/null; then
        print_msg "$RED" "Error: Python 3 is required but not installed."
        echo "Install with: sudo apt-get install python3"
        exit 1
    fi

    # Check for GDAL
    if ! python3 -c "from osgeo import gdal" &> /dev/null; then
        print_msg "$RED" "Error: GDAL Python bindings are required."
        echo "Install with: sudo apt-get install python3-gdal gdal-bin"
        exit 1
    fi

    # Check for NumPy
    if ! python3 -c "import numpy" &> /dev/null; then
        print_msg "$RED" "Error: NumPy is required."
        echo "Install with: pip3 install 'numpy<2.0'"
        exit 1
    fi
    
    # Check NumPy version (must be <2.0 for GDAL compatibility)
    if ! python3 -c "import numpy; assert numpy.__version__.startswith('1.')"; then
        print_msg "$YELLOW" "Warning: NumPy 2.x may not be compatible with GDAL."
        echo "Consider downgrading: pip3 install 'numpy<2.0'"
    fi

    # Check for Pillow
    if ! python3 -c "from PIL import Image" &> /dev/null; then
        print_msg "$RED" "Error: Pillow is required."
        echo "Install with: pip3 install pillow"
        exit 1
    fi

    print_msg "$GREEN" "✓ All prerequisites met"
}

# Build the project
build_project() {
    print_msg "$BLUE" "Building tiff-reducer..."

    cd "$PROJECT_ROOT"

    if cargo build --release --quiet 2>&1; then
        print_msg "$GREEN" "✓ Build successful"
    else
        print_msg "$RED" "✗ Build failed"
        exit 1
    fi

    if [[ ! -f "$BINARY_PATH" ]]; then
        print_msg "$RED" "Error: Binary not found at $BINARY_PATH"
        exit 1
    fi
}

# Generate the report
generate_report() {
    print_msg "$BLUE" "Generating HTML Visual Test Report..."
    print_msg "$YELLOW" "  Images: $NUM_IMAGES"
    print_msg "$YELLOW" "  Format: $FORMAT"
    print_msg "$YELLOW" "  Level:  $LEVEL"
    print_msg "$YELLOW" "  Output: $OUTPUT_DIR"
    echo ""

    cd "$PROJECT_ROOT"

    python3 "$SCRIPT_DIR/generate_html_report.py" \
        --input "$PROJECT_ROOT/tests/images" \
        --output "$OUTPUT_DIR" \
        --binary "$BINARY_PATH" \
        --format "$FORMAT" \
        --level "$LEVEL" \
        --limit "$NUM_IMAGES"

    if [[ -f "$OUTPUT_DIR/index.html" ]]; then
        print_msg "$GREEN" "✓ Report generated: $OUTPUT_DIR/index.html"
    else
        print_msg "$RED" "✗ Report generation failed"
        exit 1
    fi
}

# Open in browser
open_in_browser() {
    if [[ "$OPEN_BROWSER" == true ]]; then
        print_msg "$BLUE" "Opening report in browser..."

        if command -v xdg-open &> /dev/null; then
            xdg-open "$OUTPUT_DIR/index.html"
        elif command -v open &> /dev/null; then
            open "$OUTPUT_DIR/index.html"
        else
            print_msg "$YELLOW" "Cannot auto-open. Please open manually:"
            echo "  file://$(realpath "$OUTPUT_DIR/index.html")"
        fi
    fi
}

# Main
main() {
    echo ""
    print_msg "$BLUE" "========================================"
    print_msg "$BLUE" "  tiff-reducer HTML Test Report Generator"
    print_msg "$BLUE" "========================================"
    echo ""

    check_prerequisites
    build_project
    generate_report
    open_in_browser

    echo ""
    print_msg "$GREEN" "Done!"
    echo ""
}

main
