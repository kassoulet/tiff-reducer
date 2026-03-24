//! Build script for tiff-reducer
//!
//! # Features
//! - **vendored** (default): Builds libtiff and all dependencies from source using git submodules.
//!   Produces a fully static binary with no external dependencies.
//! - **system**: Uses system-installed libtiff and libgeotiff via pkg-config.
//!   Produces a dynamically linked binary.
//!
//! # Vendored Dependencies (from git)
//! - zlib (v1.3.1)
//! - libjpeg-turbo (3.1.0)
//! - libdeflate (v1.22)
//! - libzstd (v1.5.6)
//! - liblzma/xz (v5.6.3)
//! - libtiff (v4.7.1)

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    // Vendored is now the default; system feature opts out
    let is_vendored = !cfg!(feature = "system");

    // Try system libraries first if system feature is enabled
    if !is_vendored {
        build_system(&target);
        return;
    }

    // Default: vendored static build from git sources
    build_fully_static(&out_dir, &target);
}

fn build_system(_target: &str) {
    // Try pkg-config first (system libraries)
    let mut config = pkg_config::Config::new();
    config.atleast_version("4.0");

    if config.probe("libtiff-4").is_ok() {
        // Also link libgeotiff for GeoTIFF tag support
        if pkg_config::Config::new()
            .atleast_version("1.4")
            .probe("libgeotiff")
            .is_ok()
        {
            println!("cargo:rustc-link-lib=geotiff");
        }
        return;
    }

    // Fallback: manual linking
    println!("cargo:rustc-link-lib=tiff");
    // Link libgeotiff if available for GeoTIFF support
    println!("cargo:rustc-link-lib=geotiff");
}

fn build_fully_static(out_dir: &Path, target: &str) {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let lib_dir = out_dir.join("lib");
    let include_dir = out_dir.join("include");

    std::fs::create_dir_all(&lib_dir).unwrap();
    std::fs::create_dir_all(&include_dir).unwrap();

    // Build all dependencies in order
    println!("cargo:warning=Building zlib...");
    build_zlib(out_dir, &manifest_dir, &lib_dir, &include_dir, target);

    println!("cargo:warning=Building libjpeg-turbo...");
    build_libjpeg(out_dir, &manifest_dir, &lib_dir, &include_dir, target);

    println!("cargo:warning=Building libdeflate...");
    build_libdeflate(out_dir, &manifest_dir, &lib_dir, &include_dir, target);

    println!("cargo:warning=Building libzstd...");
    build_libzstd(out_dir, &manifest_dir, &lib_dir, &include_dir, target);

    println!("cargo:warning=Building liblzma...");
    build_liblzma(out_dir, &manifest_dir, &lib_dir, &include_dir, target);

    println!("cargo:warning=Building libtiff...");
    build_libtiff(out_dir, &manifest_dir, &lib_dir, &include_dir, target);

    // Link all static libraries
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=tiff");
    println!("cargo:rustc-link-lib=static=tiffxx");
    println!("cargo:rustc-link-lib=static=deflate");
    println!("cargo:rustc-link-lib=static=z");
    println!("cargo:rustc-link-lib=static=jpeg");
    println!("cargo:rustc-link-lib=static=zstd");
    println!("cargo:rustc-link-lib=static=lzma");
}

fn clone_or_use(out_dir: &Path, manifest_dir: &Path, name: &str, url: &str, tag: &str) -> PathBuf {
    let submodule_dir = manifest_dir.join("vendor").join(name);

    if submodule_dir.exists() {
        println!("cargo:rerun-if-changed={}", submodule_dir.display());
        submodule_dir
    } else {
        let clone_dir = out_dir.join(name);
        if !clone_dir.exists() {
            println!("cargo:warning={} not found, cloning from git...", name);
            let status = Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "--branch",
                    tag,
                    url,
                    clone_dir.to_str().unwrap(),
                ])
                .status()
                .unwrap_or_else(|_| panic!("Failed to clone {}", name));

            if !status.success() {
                panic!("Failed to clone {}", name);
            }
        }
        println!("cargo:rerun-if-changed={}", clone_dir.display());
        clone_dir
    }
}

fn build_zlib(
    out_dir: &Path,
    manifest_dir: &Path,
    lib_dir: &Path,
    include_dir: &Path,
    target: &str,
) {
    let source_dir = clone_or_use(
        out_dir,
        manifest_dir,
        "zlib",
        "https://github.com/madler/zlib.git",
        "v1.3.1",
    );

    let mut cfg = cmake::Config::new(&source_dir);
    cfg.define("BUILD_SHARED_LIBS", "OFF")
        .define("SKIP_INSTALL_LIBRARIES", "OFF")
        .define("INSTALL_LIB_DIR", lib_dir.display().to_string())
        .define("INSTALL_INCLUDE_DIR", include_dir.display().to_string());

    if target.contains("musl") {
        cfg.cflag("-static");
    }

    cfg.build();
}

fn build_libjpeg(
    out_dir: &Path,
    manifest_dir: &Path,
    _lib_dir: &Path,
    _include_dir: &Path,
    target: &str,
) {
    let source_dir = clone_or_use(
        out_dir,
        manifest_dir,
        "libjpeg-turbo",
        "https://github.com/libjpeg-turbo/libjpeg-turbo.git",
        "3.1.0",
    );

    let mut cfg = cmake::Config::new(&source_dir);
    cfg.define("BUILD_SHARED_LIBS", "OFF")
        .define("WITH_TURBOJPEG", "OFF")
        .define("ENABLE_STATIC", "ON")
        .define("ENABLE_SHARED", "OFF")
        .define("CMAKE_INSTALL_LIBDIR", "lib")
        .define("CMAKE_INSTALL_INCLUDEDIR", "include")
        // Enable SIMD optimizations
        .define("WITH_SIMD", "ON");

    // Platform-specific SIMD flags
    if target.contains("x86_64") {
        cfg.cflag("-mssse3").cflag("-msse4.2").cflag("-mavx2");
    } else if target.contains("aarch64") {
        cfg.cflag("-march=armv8-a+simd");
    }

    if target.contains("musl") {
        cfg.cflag("-static");
    }

    cfg.build();
}

fn build_libdeflate(
    out_dir: &Path,
    manifest_dir: &Path,
    _lib_dir: &Path,
    _include_dir: &Path,
    target: &str,
) {
    let source_dir = clone_or_use(
        out_dir,
        manifest_dir,
        "libdeflate",
        "https://github.com/ebiggers/libdeflate.git",
        "v1.22",
    );

    let mut cfg = cmake::Config::new(&source_dir);
    cfg.define("BUILD_SHARED_LIBS", "OFF")
        .define("LIBDEFLATE_BUILD_TESTS", "OFF")
        .define("LIBDEFLATE_BUILD_STATIC_LIB", "ON")
        .define("CMAKE_INSTALL_LIBDIR", "lib")
        .define("CMAKE_INSTALL_INCLUDEDIR", "include")
        // Enable SIMD optimizations for deflate
        .define("LIBDEFLATE_BUILD_STATIC_LIB", "ON");

    // Platform-specific SIMD flags for libdeflate
    if target.contains("x86_64") {
        cfg.cflag("-msse4.2").cflag("-mpclmul").cflag("-mavx2");
    } else if target.contains("aarch64") {
        cfg.cflag("-march=armv8-a+crc+crypto");
    }

    if target.contains("musl") {
        cfg.cflag("-static");
    }

    cfg.build();
}

fn build_libzstd(
    out_dir: &Path,
    manifest_dir: &Path,
    _lib_dir: &Path,
    _include_dir: &Path,
    target: &str,
) {
    let source_dir = clone_or_use(
        out_dir,
        manifest_dir,
        "zstd",
        "https://github.com/facebook/zstd.git",
        "v1.5.6",
    );

    let mut cfg = cmake::Config::new(source_dir.join("build/cmake"));
    cfg.define("ZSTD_BUILD_SHARED", "OFF")
        .define("ZSTD_BUILD_STATIC", "ON")
        .define("ZSTD_BUILD_PROGRAMS", "OFF")
        .define("ZSTD_BUILD_TESTS", "OFF")
        .define("ZSTD_BUILD_CONTRIB", "OFF")
        .define("CMAKE_INSTALL_LIBDIR", "lib")
        .define("CMAKE_INSTALL_INCLUDEDIR", "include")
        // Enable SIMD optimizations for zstd
        .define("ZSTD_LEGACY_SUPPORT", "OFF");

    // Platform-specific SIMD flags for zstd
    if target.contains("x86_64") {
        cfg.cflag("-msse4.2").cflag("-mavx2");
    }

    if target.contains("musl") {
        cfg.cflag("-static");
    }

    cfg.build();
}

fn build_liblzma(
    out_dir: &Path,
    manifest_dir: &Path,
    _lib_dir: &Path,
    _include_dir: &Path,
    target: &str,
) {
    let source_dir = clone_or_use(
        out_dir,
        manifest_dir,
        "xz",
        "https://github.com/tukaani-project/xz.git",
        "v5.6.3",
    );

    let mut cfg = cmake::Config::new(&source_dir);
    cfg.define("BUILD_SHARED_LIBS", "OFF")
        .define("XZ_UTILS", "OFF")
        .define("XZ_DEC", "ON")
        .define("XZ_ENC", "ON")
        .define("CMAKE_INSTALL_LIBDIR", "lib")
        .define("CMAKE_INSTALL_INCLUDEDIR", "include");

    if target.contains("musl") {
        cfg.cflag("-static");
    }

    cfg.build();
}

fn build_libtiff(
    out_dir: &Path,
    manifest_dir: &Path,
    lib_dir: &Path,
    include_dir: &Path,
    target: &str,
) {
    let source_dir = clone_or_use(
        out_dir,
        manifest_dir,
        "libtiff",
        "https://gitlab.com/libtiff/libtiff.git",
        "v4.7.1",
    );

    let mut cfg = cmake::Config::new(&source_dir);
    cfg.define("tiff-tools", "OFF")
        .define("tiff-tests", "OFF")
        .define("tiff-contrib", "OFF")
        .define("tiff-docs", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("jbig", "OFF")
        .define("webp", "OFF")
        .define("lerc", "OFF")
        .define("lzma", "ON")
        .define("zlib", "ON")
        .define("jpeg", "ON")
        .define("zstd", "ON")
        .define("jpeg12", "OFF")
        .define("ld-version-script", "OFF")
        // Point to our vendored libraries
        .define("ZLIB_INCLUDE_DIR", include_dir.display().to_string())
        .define("ZLIB_LIBRARY", lib_dir.join("libz.a").display().to_string())
        .define("JPEG_INCLUDE_DIR", include_dir.display().to_string())
        .define(
            "JPEG_LIBRARY",
            lib_dir.join("libjpeg.a").display().to_string(),
        )
        .define("CMAKE_PREFIX_PATH", out_dir.display().to_string())
        .define("CMAKE_INSTALL_LIBDIR", "lib")
        .define("CMAKE_INSTALL_INCLUDEDIR", "include");

    if target.contains("musl") {
        cfg.cflag("-static");
    }

    cfg.build();
}
