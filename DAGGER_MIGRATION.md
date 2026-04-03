# Dagger CI Migration Guide

This document explains how the GitHub Actions CI workflows have been converted to use [Dagger](https://dagger.io).

## What is Dagger?

Dagger is a portable devkit for CI/CD pipelines. It allows you to define your entire CI/CD pipeline as code, which can be run locally or in any CI system. This ensures consistency between local development and CI environments.

## Migration Overview

The following workflows have been converted to Dagger:

| Original Workflow | Dagger Workflow |
|------------------|-----------------|
| `.github/workflows/ci.yml` | `.github/workflows/ci-dagger.yml` |
| `.github/workflows/test.yml` | `.github/workflows/test-dagger.yml` |
| `.github/workflows/release.yml` | `.github/workflows/release-dagger.yml` |

## File Structure

```
dagger/
├── ts/
│   └── index.ts          # Main Dagger module with all pipeline functions
├── package.json          # Node.js dependencies
├── tsconfig.json         # TypeScript configuration
├── README.md             # Dagger-specific documentation
└── .gitignore            # Ignore node_modules and build artifacts

dagger.json               # Dagger module configuration
.github/workflows/*-dagger.yml  # GitHub Actions workflows using Dagger
```

## Available Functions

The Dagger module exposes the following functions:

### Build Functions
- `build()` - Build the project in debug mode
- `buildRelease()` - Build the project in release mode
- `buildStatic()` - Build static binary using musl
- `buildWithUpx()` - Build and compress with UPX

### Test Functions
- `test()` - Run cargo test
- `checkFormat()` - Run cargo fmt check
- `clippy()` - Run cargo clippy
- `testErrorHandling()` - Run integration tests for error handling
- `testUpxCompression()` - Test UPX compression
- `ci()` - Run the full CI suite (build + format + clippy + tests)

### Report Functions
- `generateMarkdownReport()` - Generate Markdown test report
- `generateHtmlReport()` - Generate HTML visual test report

## Running Locally

You can now run the same pipeline locally that runs in CI:

```bash
# Install Dagger CLI first
curl -L https://dl.dagger.io/dagger/install.sh | sh
sudo mv bin/dagger /usr/local/bin/

# Navigate to dagger directory
cd dagger

# Install dependencies
npm install

# Run full CI suite
dagger call ci

# Or run individual functions
dagger call build
dagger call test
dagger call clippy
```

## Benefits of Dagger

1. **Consistency**: Same pipeline runs locally and in CI
2. **Reproducibility**: Docker-based execution ensures consistent environments
3. **Portability**: Can run on any CI system or locally
4. **Caching**: Dagger intelligently caches build steps
5. **Type Safety**: TypeScript SDK provides type checking and IDE support

## GitHub Actions Integration

The GitHub Actions workflows now simply:
1. Install the Dagger CLI
2. Run the appropriate Dagger function
3. Upload artifacts if needed

This makes the workflows much simpler and easier to maintain.

## Backward Compatibility

The original GitHub Actions workflows (`ci.yml`, `test.yml`, `release.yml`) are still present in the repository for reference. Once the Dagger workflows are verified to work correctly, the original workflows can be removed.

## Troubleshooting

### Dagger engine not running
If you get an error about the Dagger engine, it will auto-start when you run any Dagger command.

### Missing dependencies
All dependencies are defined in the Dagger module and installed in the container. You don't need to install anything locally except Dagger itself.

### Slow first run
The first run will be slow as it needs to pull the Docker images. Subsequent runs will be much faster due to caching.

## Next Steps

1. Test the Dagger workflows locally
2. Verify they produce the same results as the original workflows
3. Merge this branch
4. Optionally remove the old `.github/workflows/*.yml` files (keeping only the `-dagger.yml` versions)
