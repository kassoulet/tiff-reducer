# Dagger Pipeline for tiff-reducer

This directory contains the [Dagger](https://dagger.io) pipeline for building, testing, and releasing the tiff-reducer project.

## Prerequisites

- [Dagger CLI](https://docs.dagger.io/install) (v0.18.0+)
- Node.js 18+
- Docker or compatible container runtime

## Available Commands

### Full CI Pipeline
```bash
cd dagger
npm install
dagger call ci
```

### Individual Commands

```bash
# Build (debug)
dagger call build

# Build (release)
dagger call build-release

# Run tests
dagger call test

# Check code format
dagger call check-format

# Run clippy
dagger call clippy

# Test error handling
dagger call test-error-handling

# Test UPX compression
dagger call test-upx-compression

# Generate markdown report
dagger call generate-markdown-report

# Generate HTML report
dagger call generate-html-report

# Build static binary
dagger call build-static

# Build with UPX compression
dagger call build-with-upx
```

## Pipeline Structure

- `ts/index.ts`: Main Dagger module with all pipeline functions
- `package.json`: Node.js dependencies
- `tsconfig.json`: TypeScript configuration

## GitHub Actions Integration

The workflow files in `.github/workflows/*-dagger.yml` use the Dagger CLI to run the same pipeline locally and in CI, ensuring consistency.
