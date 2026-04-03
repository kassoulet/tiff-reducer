// Dagger pipeline for tiff-reducer (libtiff-rs)
//
// This module provides portable CI/CD functions for building, testing,
// and releasing the tiff-reducer project using Dagger.

package main

import (
	"context"
	"dagger/libtiff-rs/internal/dagger"
)

type LibtiffRs struct{}

// Returns a container with all system dependencies installed
func (m *LibtiffRs) baseContainer() *dagger.Container {
	return dag.Container().
		From("rust:latest").
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{
			"apt-get", "install", "-y",
			"git",
			"cmake",
			"make",
			"g++",
			"libtiff-dev",
			"libzstd-dev",
			"liblzma-dev",
			"libjpeg-dev",
			"libwebp-dev",
			"libdeflate-dev",
			"libgeotiff-dev",
			"python3",
			"python3-pip",
			"python3-gdal",
			"gdal-bin",
			"libgdal-dev",
			"upx-ucl",
		})
}

// Returns a container with the project source code mounted
func (m *LibtiffRs) withSource(source *dagger.Directory) *dagger.Container {
	return m.baseContainer().
		WithDirectory("/usr/src/tiff-reducer", source).
		WithWorkdir("/usr/src/tiff-reducer")
}

// Build the project in debug mode
func (m *LibtiffRs) Build(source *dagger.Directory) *dagger.Container {
	return m.withSource(source).
		WithExec([]string{"cargo", "build", "--verbose"})
}

// Build the project in release mode
func (m *LibtiffRs) BuildRelease(source *dagger.Directory) *dagger.Container {
	return m.withSource(source).
		WithExec([]string{"cargo", "build", "--release"})
}

// Run cargo test
func (m *LibtiffRs) Test(ctx context.Context, source *dagger.Directory) (string, error) {
	return m.withSource(source).
		WithExec([]string{"cargo", "test", "--verbose"}).
		Stdout(ctx)
}

// Run cargo fmt check
func (m *LibtiffRs) CheckFormat(ctx context.Context, source *dagger.Directory) (string, error) {
	return m.withSource(source).
		WithExec([]string{"cargo", "fmt", "--", "--check"}).
		Stdout(ctx)
}

// Run cargo clippy
func (m *LibtiffRs) Clippy(ctx context.Context, source *dagger.Directory) (string, error) {
	return m.withSource(source).
		WithExec([]string{"cargo", "clippy", "--", "-D", "warnings"}).
		Stdout(ctx)
}

// Run integration tests for error handling
func (m *LibtiffRs) TestErrorHandling(ctx context.Context, source *dagger.Directory) (string, error) {
	return m.withSource(source).
		WithExec([]string{"cargo", "test", "--test", "integration_tests", "handling", "--verbose"}).
		Stdout(ctx)
}

// Test UPX compression (if available)
func (m *LibtiffRs) TestUpxCompression(ctx context.Context, source *dagger.Directory) (string, error) {
	_, err := m.withSource(source).
		WithExec([]string{"cargo", "build"}).
		WithExec([]string{"cp", "./target/debug/tiff-reducer", "./tiff-reducer-upx-test"}).
		WithExec([]string{"upx", "--best", "--lzma", "./tiff-reducer-upx-test"}).
		Sync(ctx)
	
	if err != nil {
		return "", err
	}
	
	return "UPX compression test successful", nil
}

// Generate Markdown test report
func (m *LibtiffRs) GenerateMarkdownReport(source *dagger.Directory) *dagger.Directory {
	return m.BuildRelease(source).
		WithExec([]string{"./tests/generate-report.sh", "--format", "zstd", "--level", "19", "--number", "20"}).
		Directory("/usr/src/tiff-reducer/tests/report")
}

// Generate HTML visual test report
func (m *LibtiffRs) GenerateHtmlReport(source *dagger.Directory) *dagger.Directory {
	return m.BuildRelease(source).
		WithExec([]string{"pip3", "install", "--break-system-packages", "numpy", "pillow"}).
		WithExec([]string{
			"python3", "tests/generate_html_report.py",
			"--input", "tests/images",
			"--output", "tests/report",
			"--binary", "./target/release/tiff-reducer",
			"--format", "zstd",
			"--level", "19",
			"--limit", "20",
		}).
		Directory("/usr/src/tiff-reducer/tests/report")
}

// Build static binary using musl
func (m *LibtiffRs) BuildStatic(source *dagger.Directory) *dagger.Container {
	return dag.Container().
		From("rust:alpine").
		WithExec([]string{"apk", "add", "--no-cache",
			"cmake",
			"make",
			"g++",
			"git",
			"musl-dev",
			"musl-fts-dev",
			"musl-libintl",
			"linux-headers",
			"perl",
			"py3-pyelftools",
		}).
		WithDirectory("/usr/src/tiff-reducer", source).
		WithWorkdir("/usr/src/tiff-reducer").
		WithExec([]string{"cargo", "build", "--release", "--features", "vendored", "--target", "x86_64-unknown-linux-musl"})
}

// Get the static binary file
func (m *LibtiffRs) GetStaticBinary(source *dagger.Directory) *dagger.File {
	return m.BuildStatic(source).
		File("/usr/src/tiff-reducer/target/x86_64-unknown-linux-musl/release/tiff-reducer")
}

// Build and compress with UPX
func (m *LibtiffRs) BuildWithUpx(source *dagger.Directory) *dagger.File {
	return m.withSource(source).
		WithExec([]string{"cargo", "build", "--release"}).
		WithExec([]string{"cp", "./target/release/tiff-reducer", "./tiff-reducer-compressed"}).
		WithExec([]string{"upx", "--best", "--lzma", "./tiff-reducer-compressed"}).
		File("./tiff-reducer-compressed")
}

// Run the full CI suite
func (m *LibtiffRs) Ci(ctx context.Context, source *dagger.Directory) (string, error) {
	// Build
	if _, err := m.Build(source).Sync(ctx); err != nil {
		return "", err
	}
	
	// Format check
	if _, err := m.CheckFormat(ctx, source); err != nil {
		return "", err
	}
	
	// Clippy
	if _, err := m.Clippy(ctx, source); err != nil {
		return "", err
	}
	
	// Tests
	if _, err := m.Test(ctx, source); err != nil {
		return "", err
	}
	
	// Error handling tests
	if _, err := m.TestErrorHandling(ctx, source); err != nil {
		return "", err
	}
	
	return "All CI checks passed!", nil
}
