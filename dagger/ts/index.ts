import { dag, Container, Directory, File, object, func } from "@dagger.io/dagger"

@object()
export class TiffReducerPipeline {
  /**
   * Returns a container with all system dependencies installed
   */
  @func()
  baseContainer(): Container {
    return dag
      .container()
      .from("rust:latest")
      .withExec(["apt-get", "update"])
      .withExec([
        "apt-get", "install", "-y",
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
      ])
  }

  /**
   * Returns a container with the project source code mounted
   */
  @func()
  withSource(): Container {
    const source = dag.host().directory(".", { exclude: ["node_modules/", "target/", "dagger/node_modules/"] })
    return this.baseContainer()
      .withDirectory("/usr/src/tiff-reducer", source)
  }

  /**
   * Build the project in debug mode
   */
  @func()
  async build(): Promise<Container> {
    return this.withSource()
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["cargo", "build", "--verbose"])
  }

  /**
   * Build the project in release mode
   */
  @func()
  async buildRelease(): Promise<Container> {
    return this.withSource()
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["cargo", "build", "--release"])
  }

  /**
   * Run cargo test
   */
  @func()
  async test(): Promise<string> {
    return this.withSource()
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["cargo", "test", "--verbose"])
      .stdout()
  }

  /**
   * Run cargo fmt check
   */
  @func()
  async checkFormat(): Promise<string> {
    return this.withSource()
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["cargo", "fmt", "--", "--check"])
      .stdout()
  }

  /**
   * Run cargo clippy
   */
  @func()
  async clippy(): Promise<string> {
    return this.withSource()
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["cargo", "clippy", "--", "-D", "warnings"])
      .stdout()
  }

  /**
   * Run integration tests for error handling
   */
  @func()
  async testErrorHandling(): Promise<string> {
    return this.withSource()
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["cargo", "test", "--test", "integration_tests", "handling", "--verbose"])
      .stdout()
  }

  /**
   * Test UPX compression (if available)
   */
  @func()
  async testUpxCompression(): Promise<string> {
    const container = this.withSource()
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["cargo", "build"])
      .withExec(["cp", "./target/debug/tiff-reducer", "./tiff-reducer-upx-test"])
      .withExec(["upx", "--best", "--lzma", "./tiff-reducer-upx-test"])

    return "UPX compression test successful"
  }

  /**
   * Generate Markdown test report
   */
  @func()
  generateMarkdownReport(): Directory {
    return this.buildRelease()
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["./tests/generate-report.sh", "--format", "zstd", "--level", "19", "--number", "20"])
      .directory("/usr/src/tiff-reducer/tests/report")
  }

  /**
   * Generate HTML visual test report
   */
  @func()
  generateHtmlReport(): Directory {
    return this.buildRelease()
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["pip3", "install", "--break-system-packages", "numpy", "pillow"])
      .withExec([
        "python3", "tests/generate_html_report.py",
        "--input", "tests/images",
        "--output", "tests/report",
        "--binary", "./target/release/tiff-reducer",
        "--format", "zstd",
        "--level", "19",
        "--limit", "20",
      ])
      .directory("/usr/src/tiff-reducer/tests/report")
  }

  /**
   * Build static binary using musl
   */
  @func()
  async buildStatic(): Promise<Container> {
    const source = dag.host().directory(".", { exclude: ["node_modules/", "target/", "dagger/node_modules/"] })
    const builder = dag
      .container()
      .from("rust:alpine")
      .withExec(["apk", "add", "--no-cache",
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
      ])
      .withDirectory("/usr/src/tiff-reducer", source)
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["cargo", "build", "--release", "--features", "vendored", "--target", "x86_64-unknown-linux-musl"])

    return builder
  }

  /**
   * Get the static binary file
   */
  @func()
  getStaticBinary(): File {
    return this.buildStatic()
      .file("/usr/src/tiff-reducer/target/x86_64-unknown-linux-musl/release/tiff-reducer")
  }

  /**
   * Build and compress with UPX
   */
  @func()
  async buildWithUpx(): Promise<File> {
    const container = this.withSource()
      .withWorkdir("/usr/src/tiff-reducer")
      .withExec(["cargo", "build", "--release"])
      .withExec(["cp", "./target/release/tiff-reducer", "./tiff-reducer-compressed"])
      .withExec(["upx", "--best", "--lzma", "./tiff-reducer-compressed"])

    return container.file("./tiff-reducer-compressed")
  }

  /**
   * Run the full CI suite
   */
  @func()
  async ci(): Promise<string> {
    // Build
    await this.build()
    console.log("✓ Build successful")

    // Format check
    await this.checkFormat()
    console.log("✓ Format check passed")

    // Clippy
    await this.clippy()
    console.log("✓ Clippy passed")

    // Tests
    await this.test()
    console.log("✓ Tests passed")

    // Error handling tests
    await this.testErrorHandling()
    console.log("✓ Error handling tests passed")

    return "All CI checks passed!"
  }
}
