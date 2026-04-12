#!/bin/bash
#
# Run full test suite and generate test report for tiff-reducer
#
# This script:
# 1. Builds the project in release mode
# 2. Runs all integration tests (tests all images with zstd and uncompressed)
# 3. Generates a comprehensive Markdown test report at tests/README.md
#
# Usage:
#   ./scripts/run-tests.sh [options]
#
# Options:
#   -f, --format FORMAT    Compression format for report (default: zstd)
#   -l, --level LEVEL      Compression level for report (default: 19)
#   -n, --limit NUM        Limit number of images in report (default: all)
#   -o, --output PATH      Output path for report (default: tests/README.md)
#   -h, --help             Show this help message
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Default values
FORMAT="zstd"
LEVEL=19
LIMIT=""
OUTPUT="tests/README.md"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_msg() {
    echo -e "${1}${2}${NC}"
}

show_help() {
    head -25 "$0" | tail -20
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -f|--format)
            FORMAT="$2"
            shift 2
            ;;
        -l|--level)
            LEVEL="$2"
            shift 2
            ;;
        -n|--limit)
            LIMIT="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT="$2"
            shift 2
            ;;
        -h|--help)
            show_help
            ;;
        *)
            print_msg "$RED" "Unknown option: $1"
            exit 1
            ;;
    esac
done

cd "$PROJECT_ROOT"

# Step 1: Build
print_msg "$BLUE" "========================================"
print_msg "$BLUE" "  Building tiff-reducer..."
print_msg "$BLUE" "========================================"
cargo build --release --quiet
print_msg "$GREEN" "✓ Build successful"
echo ""

# Step 2: Run integration tests
print_msg "$BLUE" "========================================"
print_msg "$BLUE" "  Running integration tests..."
print_msg "$BLUE" "========================================"
if cargo test --test integration_tests -- --test-threads=1; then
    print_msg "$GREEN" "✓ All integration tests passed"
else
    print_msg "$RED" "✗ Integration tests failed"
    exit 1
fi
echo ""

# Step 3: Generate test report
print_msg "$BLUE" "========================================"
print_msg "$BLUE" "  Generating test report..."
print_msg "$BLUE" "========================================"
print_msg "$YELLOW" "  Format: $FORMAT"
print_msg "$YELLOW" "  Level:  $LEVEL"
if [[ -n "$LIMIT" ]]; then
    print_msg "$YELLOW" "  Limit:  $LIMIT images"
fi
print_msg "$YELLOW" "  Output: $OUTPUT"
echo ""

REPORT_CMD=(cargo run --bin test-report --release --quiet -- --format "$FORMAT" --level "$LEVEL" --output "$OUTPUT")
if [[ -n "$LIMIT" ]]; then
    REPORT_CMD+=(--limit "$LIMIT")
fi

"${REPORT_CMD[@]}"

if [[ -f "$OUTPUT" ]]; then
    print_msg "$GREEN" "✓ Report generated: $OUTPUT"
else
    print_msg "$RED" "✗ Report generation failed"
    exit 1
fi
echo ""

print_msg "$GREEN" "========================================"
print_msg "$GREEN" "  All tests passed!"
print_msg "$GREEN" "========================================"
