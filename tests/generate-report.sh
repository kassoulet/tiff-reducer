#!/bin/bash
#
# Generate Markdown Test Report for tiff-reducer
#
# This script builds the project and generates a markdown report with:
# - Summary statistics (pass/fail counts)
# - Failure breakdown by error type
# - List of working and failed images
#
# Usage:
#   ./tests/generate-report.sh [options]
#
# Options:
#   -n, --number    Number of test images to process (default: all)
#   -f, --format    Compression format (default: zstd)
#   -l, --level     Compression level (default: 19)
#   -o, --output    Output directory (default: tests/report)
#   -h, --help      Show this help message
#

set -e

# Default values
NUM_IMAGES="all"
FORMAT="zstd"
LEVEL=19
OUTPUT_DIR="tests/report"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

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
}

# Generate the report
generate_report() {
    print_msg "$BLUE" "Generating Markdown Test Report..."
    print_msg "$YELLOW" "  Format: $FORMAT"
    print_msg "$YELLOW" "  Level:  $LEVEL"
    print_msg "$YELLOW" "  Output: $OUTPUT_DIR"
    echo ""

    cd "$PROJECT_ROOT"

    # Create output directory
    mkdir -p "$OUTPUT_DIR/thumbnails"

    python3 "$SCRIPT_DIR/generate_test_report.py" \
        --input "$PROJECT_ROOT/tests/images" \
        --output "$OUTPUT_DIR" \
        --binary "$PROJECT_ROOT/target/release/tiff-reducer" \
        --format "$FORMAT" \
        --level "$LEVEL" \
        ${NUM_IMAGES:+--limit "$NUM_IMAGES"}

    if [[ -f "$OUTPUT_DIR/README.md" ]]; then
        print_msg "$GREEN" "✓ Report generated: $OUTPUT_DIR/README.md"
    else
        print_msg "$RED" "✗ Report generation failed"
        exit 1
    fi
}

# Main
main() {
    echo ""
    print_msg "$BLUE" "========================================"
    print_msg "$BLUE" "  tiff-reducer Test Report Generator"
    print_msg "$BLUE" "========================================"
    echo ""

    check_prerequisites
    build_project
    generate_report

    echo ""
    print_msg "$GREEN" "Done!"
    echo ""
}

main
