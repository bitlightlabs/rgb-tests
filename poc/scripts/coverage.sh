#!/bin/bash

# RGB Sandbox Test Coverage Script
# This script provides comprehensive coverage analysis and reporting

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
MIN_COVERAGE=50.0
OUTPUT_DIR="coverage"
REPORTS_DIR="coverage-reports"

echo -e "${BLUE}🔍 RGB Sandbox Test Coverage Analysis${NC}"
echo "=========================================="

# Check if cargo-llvm-cov is installed
if ! command -v cargo-llvm-cov &> /dev/null; then
    echo -e "${RED}❌ cargo-llvm-cov not found. Installing...${NC}"
    cargo install cargo-llvm-cov
fi

# Check if llvm-tools-preview is installed
if ! rustup component list --installed | grep -q llvm-tools-preview; then
    echo -e "${YELLOW}⚠️  Installing llvm-tools-preview...${NC}"
    rustup component add llvm-tools-preview
fi

# Create output directories
mkdir -p "$OUTPUT_DIR"
mkdir -p "$REPORTS_DIR"

echo -e "${BLUE}📊 Running tests with coverage analysis...${NC}"

# Generate coverage reports in multiple formats
echo "Generating LCOV report..."
cargo llvm-cov --all-features --lcov --output-path "$OUTPUT_DIR/coverage.lcov"

echo "Generating HTML report..."
cargo llvm-cov --all-features --html --output-dir "$OUTPUT_DIR/html"

echo "Generating JSON report..."
cargo llvm-cov --all-features --json --output-path "$OUTPUT_DIR/coverage.json"

# Get summary coverage
echo -e "${BLUE}📈 Coverage Summary${NC}"
SUMMARY=$(cargo llvm-cov --all-features --summary-only)
echo "$SUMMARY"

# Extract overall coverage percentage
COVERAGE=$(echo "$SUMMARY" | grep -E "TOTAL.*[0-9]+\.[0-9]+%" | grep -o '[0-9]\+\.[0-9]\+%' | head -1 | sed 's/%//')

if [ -z "$COVERAGE" ]; then
    echo -e "${RED}❌ Could not extract coverage percentage${NC}"
    exit 1
fi

echo -e "${BLUE}📊 Overall Coverage: ${GREEN}$COVERAGE%${NC}"

# Check against threshold
if (( $(echo "$COVERAGE < $MIN_COVERAGE" | bc -l) )); then
    echo -e "${RED}❌ Coverage $COVERAGE% is below minimum threshold $MIN_COVERAGE%${NC}"
    COVERAGE_STATUS="FAIL"
    EXIT_CODE=1
else
    echo -e "${GREEN}✅ Coverage $COVERAGE% meets minimum threshold $MIN_COVERAGE%${NC}"
    COVERAGE_STATUS="PASS"
    EXIT_CODE=0
fi

# Generate detailed report for specific modules
echo -e "${BLUE}🔍 Module-specific Coverage${NC}"
echo "=============================="

# Extract poc-specific coverage
POC_COVERAGE=$(cargo llvm-cov --all-features | grep "poc/src" || echo "No poc-specific data found")
if [ "$POC_COVERAGE" != "No poc-specific data found" ]; then
    echo "$POC_COVERAGE"
else
    echo "ℹ️  Detailed module coverage available in HTML report"
fi

# Generate coverage badge data
BADGE_COLOR=""
if (( $(echo "$COVERAGE >= 80" | bc -l) )); then
    BADGE_COLOR="brightgreen"
elif (( $(echo "$COVERAGE >= 60" | bc -l) )); then
    BADGE_COLOR="yellow"
elif (( $(echo "$COVERAGE >= 40" | bc -l) )); then
    BADGE_COLOR="orange"
else
    BADGE_COLOR="red"
fi

# Create badge URL
BADGE_URL="https://img.shields.io/badge/coverage-$COVERAGE%25-$BADGE_COLOR"
echo "Coverage Badge URL: $BADGE_URL"

# Generate timestamped report
TIMESTAMP=$(date '+%Y-%m-%d_%H-%M-%S')
REPORT_FILE="$REPORTS_DIR/coverage_report_$TIMESTAMP.txt"

cat > "$REPORT_FILE" << EOF
RGB Sandbox Coverage Report
Generated: $(date)
==========================

Overall Coverage: $COVERAGE%
Status: $COVERAGE_STATUS
Threshold: $MIN_COVERAGE%

Module Details:
$SUMMARY

Files Generated:
- HTML Report: $OUTPUT_DIR/html/index.html
- LCOV Report: $OUTPUT_DIR/coverage.lcov
- JSON Report: $OUTPUT_DIR/coverage.json

Badge URL: $BADGE_URL
EOF

echo -e "${GREEN}📄 Detailed report saved to: $REPORT_FILE${NC}"

# Show where to find reports
echo -e "${BLUE}📂 Coverage Reports Location${NC}"
echo "================================"
echo "HTML Report: file://$(pwd)/$OUTPUT_DIR/html/index.html"
echo "LCOV Report: $(pwd)/$OUTPUT_DIR/coverage.lcov"
echo "JSON Report: $(pwd)/$OUTPUT_DIR/coverage.json"
echo "Text Report: $(pwd)/$REPORT_FILE"

# Optional: Open HTML report in browser (macOS/Linux)
if command -v open &> /dev/null; then
    read -p "Open HTML report in browser? (y/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        open "$OUTPUT_DIR/html/index.html"
    fi
elif command -v xdg-open &> /dev/null; then
    read -p "Open HTML report in browser? (y/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        xdg-open "$OUTPUT_DIR/html/index.html"
    fi
fi

echo -e "${BLUE}✨ Coverage analysis complete!${NC}"

exit $EXIT_CODE