#!/bin/bash
#
# Run all tests and generate report for tiff-reducer
#
# This script:
# 1. Runs unit tests
# 2. Runs integration tests
# 3. Generates a Markdown test report at tests/README.md
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_msg() {
    local color=$1
    local msg=$2
    echo -e "${color}${msg}${NC}"
}

# Build the project
print_msg "$BLUE" "Building tiff-reducer..."
cd "$PROJECT_ROOT"
cargo build --release --quiet
print_msg "$GREEN" "✓ Build successful"

# Run unit tests
print_msg "$BLUE" "Running tests..."
if cargo test --quiet 2>&1; then
    print_msg "$GREEN" "✓ All tests passed"
else
    print_msg "$RED" "✗ Tests failed"
    exit 1
fi

# Generate test report
print_msg "$BLUE" "Generating test report..."
cargo run --bin test-report --quiet 2>&1
if [[ -f "tests/README.md" ]]; then
    print_msg "$GREEN" "✓ Report generated: tests/README.md"
else
    print_msg "$RED" "✗ Report generation failed"
    exit 1
fi

print_msg "$GREEN" "All tests passed!"
