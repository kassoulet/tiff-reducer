#![allow(clippy::collapsible_if, clippy::redundant_closure_for_method_calls)]

mod ffi;
mod metadata;
mod quantize;

use crate::ffi::*;
use crate::metadata::clone_metadata;
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand, ValueEnum};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::ffi::CString;
use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::{Component, Path, PathBuf};

/// Sanitize a filename to prevent path traversal attacks
/// Returns None if the filename contains path separators or is invalid
fn sanitize_filename(name: &std::ffi::OsStr) -> Option<String> {
    let path = Path::new(name);

    // Reject paths with parent directory components (..)
    for component in path.components() {
        if let Component::ParentDir = component {
            return None;
        }
    }

    // Reject absolute paths
    if path.is_absolute() {
        return None;
    }

    // Reject paths with any directory separators
    for component in path.components() {
        if let Component::Normal(_) = component {
            // OK - this is a normal filename component
        } else {
            return None;
        }
    }

    // Convert to string and reject if contains null bytes
    name.to_str().and_then(|s| {
        if s.contains('\0') {
            None
        } else {
            Some(s.to_string())
        }
    })
}

#[derive(Parser)]
#[command(name = "tiff-reducer")]
#[command(about = "Optimize TIFF files with high-efficiency codecs", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compress one or more TIFF files
    Compress {
        /// Input file(s) or directory
        #[arg(required = true)]
        input: Vec<PathBuf>,

        /// Output file or directory (overwrites input if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Compression format to use
        #[arg(short, long, value_enum, default_value_t = CompressionFormat::Zstd, conflicts_with = "lossy")]
        format: CompressionFormat,

        /// Compression level (Zstd: 1-22 default 19, Deflate/LZMA: 1-9, JPEG/WebP: 1-100)
        #[arg(short, long)]
        level: Option<u32>,

        /// Use lossy compression (tries WebP and JPEG, picks smallest)
        #[arg(long)]
        lossy: bool,

        /// Quantize to 8-bit
        #[arg(long)]
        quantize: bool,

        /// Try all compression formats and display a report
        #[arg(long)]
        extreme: bool,

        /// Perform compression but do not write to disk
        #[arg(long)]
        dry_run: bool,

        /// Run benchmark mode with timing and throughput metrics
        #[arg(long)]
        benchmark: bool,

        /// Number of parallel jobs (default: number of CPUs)
        #[arg(short, long)]
        jobs: Option<usize>,
    },
    /// Analyze a TIFF file and display metadata
    Analyze {
        /// Input TIFF file
        #[arg(required = true)]
        path: PathBuf,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum CompressionFormat {
    Uncompressed,
    Deflate,
    Zstd,
    Lzma,
    Lzw,
    Packbits,
    Jpeg,
    Webp,
    JpegXl,
}

impl std::fmt::Display for CompressionFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl CompressionFormat {
    fn to_ffi(self) -> u16 {
        match self {
            CompressionFormat::Uncompressed => COMPRESSION_NONE,
            CompressionFormat::Deflate => COMPRESSION_ADOBE_DEFLATE,
            CompressionFormat::Zstd => COMPRESSION_ZSTD,
            CompressionFormat::Lzma => COMPRESSION_LZMA,
            CompressionFormat::Lzw => COMPRESSION_LZW,
            CompressionFormat::Packbits => COMPRESSION_PACKBITS,
            CompressionFormat::Jpeg => COMPRESSION_JPEG,
            CompressionFormat::Webp => COMPRESSION_WEBP,
            CompressionFormat::JpegXl => COMPRESSION_JPEGXL,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Predictor {
    None,
    Horizontal,
    FloatingPoint,
}

impl std::fmt::Display for Predictor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Predictor {
    fn to_ffi(self) -> u16 {
        match self {
            Predictor::None => PREDICTOR_NONE,
            Predictor::Horizontal => PREDICTOR_HORIZONTAL,
            Predictor::FloatingPoint => PREDICTOR_FLOATINGPOINT,
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Compress {
            input,
            output,
            format,
            level,
            lossy,
            quantize,
            extreme,
            dry_run,
            benchmark,
            jobs,
        } => {
            compress_command(
                input, output, format, level, lossy, quantize, extreme, dry_run, benchmark, jobs,
            )?;
        }
        Commands::Analyze { path } => {
            analyze_command(&path)?;
        }
    }

    Ok(())
}

fn analyze_command(path: &Path) -> Result<()> {
    let c_path = CString::new(path.to_str().ok_or_else(|| anyhow!("Invalid path"))?)?;
    unsafe {
        let tif = TIFFOpen(c_path.as_ptr(), CString::new("r")?.as_ptr());
        if tif.is_null() {
            return Err(anyhow!("Failed to open TIFF file: {:?}", path));
        }

        let mut w = 0u32;
        let mut h = 0u32;
        let mut bps = 0u16;
        let mut spp = 0u16;
        let mut comp = 0u16;
        let mut fmt = SAMPLEFORMAT_UINT; // Default to uint

        // Check return values for all TIFFGetField calls
        if TIFFGetField(tif, TIFFTAG_IMAGEWIDTH, &mut w) == 0 || w == 0 {
            TIFFClose(tif);
            return Err(anyhow!("Failed to read image width"));
        }
        if TIFFGetField(tif, TIFFTAG_IMAGELENGTH, &mut h) == 0 || h == 0 {
            TIFFClose(tif);
            return Err(anyhow!("Failed to read image length"));
        }
        if TIFFGetField(tif, TIFFTAG_BITSPERSAMPLE, &mut bps) == 0 || bps == 0 {
            TIFFClose(tif);
            return Err(anyhow!("Failed to read bits per sample"));
        }
        if TIFFGetField(tif, TIFFTAG_SAMPLESPERPIXEL, &mut spp) == 0 || spp == 0 {
            TIFFClose(tif);
            return Err(anyhow!("Failed to read samples per pixel"));
        }
        TIFFGetField(tif, TIFFTAG_COMPRESSION, &mut comp);
        TIFFGetField(tif, TIFFTAG_SAMPLEFORMAT, &mut fmt);

        println!("File: {:?}", path);
        println!("Dimensions: {}x{}", w, h);
        println!("Samples: {} channels, {} bits/sample", spp, bps);
        println!(
            "Format: {}",
            match fmt {
                SAMPLEFORMAT_UINT => "Unsigned Integer",
                SAMPLEFORMAT_INT => "Signed Integer",
                SAMPLEFORMAT_IEEEFP => "Floating Point",
                _ => "Unknown",
            }
        );
        println!("Compression: {} ({})", compression_name(comp), comp);
        println!(
            "Layout: {}",
            if crate::ffi::TIFFIsTiled(tif) != 0 {
                let mut tw: u32 = 0;
                let mut th: u32 = 0;
                TIFFGetField(tif, TIFFTAG_TILEWIDTH, &mut tw);
                TIFFGetField(tif, TIFFTAG_TILELENGTH, &mut th);
                format!("Tiled ({}x{})", tw, th)
            } else {
                "Striped".to_string()
            }
        );

        TIFFClose(tif);
    }
    Ok(())
}

fn compression_name(comp: u16) -> &'static str {
    match comp {
        COMPRESSION_NONE => "Uncompressed",
        c if c == COMPRESSION_ADOBE_DEFLATE || c == COMPRESSION_DEFLATE => "Deflate",
        COMPRESSION_ZSTD => "Zstd",
        COMPRESSION_LZMA => "LZMA",
        COMPRESSION_LZW => "LZW",
        COMPRESSION_PACKBITS => "PackBits",
        COMPRESSION_JPEG => "JPEG",
        COMPRESSION_WEBP => "WebP",
        COMPRESSION_JPEGXL => "JPEG-XL",
        COMPRESSION_CCITTFAX3 => "CCITT Group 3",
        COMPRESSION_CCITTFAX4 => "CCITT Group 4",
        _ => "Unknown",
    }
}

#[allow(clippy::too_many_arguments)]
fn compress_command(
    input: Vec<PathBuf>,
    output: Option<PathBuf>,
    format: CompressionFormat,
    level: Option<u32>,
    lossy: bool,
    quantize: bool,
    extreme: bool,
    dry_run: bool,
    benchmark: bool,
    jobs: Option<usize>,
) -> Result<()> {
    // Expand directories to file lists
    let files: Vec<PathBuf> = input
        .iter()
        .flat_map(|path| {
            if path.is_dir() {
                fs::read_dir(path)
                    .unwrap()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension().is_some_and(|ext| {
                            ext == "tif" || ext == "tiff" || ext == "TIF" || ext == "TIFF"
                        })
                    })
                    .collect()
            } else {
                vec![path.clone()]
            }
        })
        .collect();

    if files.is_empty() {
        return Err(anyhow!("No TIFF files found in the specified input paths"));
    }

    let m = MultiProgress::new();

    // Use rayon for file-level parallelism with configurable job count
    let num_jobs = jobs.unwrap_or_else(num_cpus::get);

    files
        .par_iter()
        .with_max_len(num_jobs)
        .for_each(|file_path| {
            let pb = m.add(ProgressBar::new(100));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "{spinner:.green} [{elapsed_precise}] {msg} [{bar:40.cyan/blue}] {pos}%",
                    )
                    .unwrap(),
            );
            pb.set_position(0);
            pb.set_message(format!(
                "Processing {:?}",
                file_path.file_name().unwrap_or(file_path.as_os_str())
            ));

            let target_output = if let Some(ref out) = output {
                if out.is_dir() {
                    // Sanitize filename to prevent path traversal attacks
                    match sanitize_filename(file_path.file_name().unwrap_or(file_path.as_os_str()))
                    {
                        Some(safe_name) => out.join(safe_name),
                        None => {
                            pb.finish_with_message(format!(
                                "Error: Invalid filename {:?}",
                                file_path.file_name()
                            ));
                            return;
                        }
                    }
                } else {
                    out.clone()
                }
            } else {
                file_path.clone()
            };

            match process_single_file(
                file_path,
                &target_output,
                output.is_some(),
                format,
                level,
                lossy,
                quantize,
                extreme,
                dry_run,
                benchmark,
                &pb,
            ) {
                Ok((original, compressed, best_fmt, is_dry_run)) => {
                    pb.finish();
                    if !extreme && !lossy {
                        let ratio = if original > 0 {
                            (1.0 - (compressed as f64 / original as f64)) * 100.0
                        } else {
                            0.0
                        };
                        println!(
                            "\n[{}] {}: {} -> {} bytes ({:.1}% reduction, {})",
                            file_path
                                .file_name()
                                .unwrap_or(file_path.as_os_str())
                                .to_string_lossy(),
                            if is_dry_run { "Dry-run" } else { "Final" },
                            original,
                            compressed,
                            ratio,
                            best_fmt
                        );
                    }
                }
                Err(e) => {
                    pb.finish_with_message(format!("Error: {}", e));
                }
            }
        });

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_single_file(
    input: &Path,
    output: &Path,
    has_explicit_output: bool,
    format: CompressionFormat,
    level: Option<u32>,
    lossy: bool,
    quantize: bool,
    extreme: bool,
    dry_run: bool,
    benchmark: bool,
    pb: &ProgressBar,
) -> Result<(u64, u64, String, bool)> {
    let original_size = fs::metadata(input)?.len();
    let start_time = std::time::Instant::now();

    // Get TIFF info to decide on quantization
    let (_w, _h, bps, _spp, sample_format) = get_tiff_info(input)?;
    let is_float = sample_format == SAMPLEFORMAT_IEEEFP;

    // Automatically enable quantization for lossy mode if bps > 8
    let quantize = quantize || (lossy && bps > 8);

    let formats = if extreme {
        vec![
            CompressionFormat::Uncompressed,
            CompressionFormat::Zstd,
            CompressionFormat::Lzma,
            CompressionFormat::Deflate,
            CompressionFormat::JpegXl,
        ]
    } else if lossy {
        vec![CompressionFormat::Webp, CompressionFormat::Jpeg]
    } else {
        vec![format]
    };

    // Default level for lossy compression if not specified
    let effective_level = if level.is_none()
        && (lossy || matches!(format, CompressionFormat::Webp | CompressionFormat::Jpeg))
    {
        Some(90)
    } else {
        level
    };

    // Predictors to test (skip for lossy formats)
    let predictors = if extreme {
        if is_float {
            vec![
                Predictor::None,
                Predictor::Horizontal,
                Predictor::FloatingPoint,
            ]
        } else {
            // For integer data, only test None and Horizontal
            vec![Predictor::None, Predictor::Horizontal]
        }
    } else if lossy {
        vec![Predictor::None]
    } else {
        vec![Predictor::Horizontal] // default
    };

    let mut best_format = formats[0];
    let mut best_predictor = predictors[0];
    let mut best_size = u64::MAX;
    let mut results: Vec<(CompressionFormat, Predictor, u64)> = Vec::new();

    let should_benchmark = extreme || (lossy && formats.len() > 1);

    if should_benchmark {
        pb.set_message(format!(
            "Benchmarking formats for {:?}",
            input.file_name().unwrap_or(input.as_os_str())
        ));

        let mut combinations = Vec::new();
        for &fmt in &formats {
            for &pred in &predictors {
                // Skip predictors for lossy compression (JPEG, WebP)
                if matches!(fmt, CompressionFormat::Jpeg | CompressionFormat::Webp)
                    && pred != Predictor::None
                {
                    continue;
                }
                combinations.push((fmt, pred));
            }
        }

        let total = combinations.len();
        for (i, (fmt, pred)) in combinations.iter().enumerate() {
            let temp_file = tempfile::tempfile()?;
            let cid = fmt.to_ffi();
            let pid = pred.to_ffi();

            // Only add to results if compression actually succeeded
            if let Ok(size) =
                run_compression_to_fd(input, temp_file, cid, pid, effective_level, quantize)
            {
                if size > 0 && size < u64::MAX {
                    results.push((*fmt, *pred, size));
                    if size < best_size {
                        best_size = size;
                        best_format = *fmt;
                        best_predictor = *pred;
                    }
                }
            }

            // Update progress
            let progress = ((i + 1) as u64 * 100) / total as u64;
            pb.set_position(progress);
            pb.set_message(format!(
                "Benchmarking: {}/{} combinations tested",
                i + 1,
                total
            ));
        }

        // Display results for each combination
        println!(
            "\n[{}] Compression results:",
            input
                .file_name()
                .unwrap_or(input.as_os_str())
                .to_string_lossy()
        );
        for (fmt, pred, size) in &results {
            let ratio = if original_size > 0 {
                (1.0 - (*size as f64 / original_size as f64)) * 100.0
            } else {
                0.0
            };
            let marker = if *fmt == best_format && *pred == best_predictor {
                "✓"
            } else {
                " "
            };
            println!(
                "  [{}] {:<10} {:<10} {} bytes ({:.1}% reduction)",
                marker, fmt, pred, size, ratio
            );
        }
        pb.set_message(format!(
            "Winner: {} + {} ({} bytes)",
            best_format, best_predictor, best_size
        ));
    } else {
        pb.set_message(format!(
            "Compressing {:?}",
            input.file_name().unwrap_or(input.as_os_str())
        ));
    }

    if dry_run {
        if has_explicit_output {
            pb.println("Warning: --output is ignored when using --dry-run");
        }

        let temp_file = tempfile::tempfile()?;
        let cid = best_format.to_ffi();
        let pid = best_predictor.to_ffi();

        let dry_run_size =
            run_compression_to_fd(input, temp_file, cid, pid, effective_level, quantize)?;

        return Ok((
            original_size,
            dry_run_size,
            format!("{best_format}+{best_predictor}"),
            true,
        ));
    }

    // Final compression with best format and predictor
    let cid = best_format.to_ffi();
    let pid = best_predictor.to_ffi();
    run_compression_pass(input, output, cid, pid, effective_level, quantize)?;

    let compressed_size = fs::metadata(output)?.len();
    let elapsed = start_time.elapsed();

    // Display benchmark results if requested
    if benchmark {
        let throughput_mbs = if elapsed.as_secs_f64() > 0.0 {
            (original_size as f64 / 1048576.0) / elapsed.as_secs_f64()
        } else {
            0.0
        };
        let ratio = if original_size > 0 {
            (1.0 - (compressed_size as f64 / original_size as f64)) * 100.0
        } else {
            0.0
        };
        println!(
            "\n[{}] Benchmark Results:",
            input
                .file_name()
                .unwrap_or(input.as_os_str())
                .to_string_lossy()
        );
        println!("  Original size:   {} bytes", original_size);
        println!("  Compressed size: {} bytes", compressed_size);
        println!("  Compression:     {:.1}% reduction", ratio);
        println!("  Time elapsed:    {:.3}s", elapsed.as_secs_f64());
        println!("  Throughput:      {:.2} MB/s", throughput_mbs);
    }

    Ok((
        original_size,
        compressed_size,
        format!("{best_format}+{best_predictor}"),
        false,
    ))
}

/// Get basic info about a TIFF file
fn get_tiff_info(path: &Path) -> Result<(u32, u32, u16, u16, u16)> {
    let c_path = CString::new(path.to_str().ok_or_else(|| anyhow!("Invalid path"))?)?;
    unsafe {
        let tif = TIFFOpen(c_path.as_ptr(), CString::new("r")?.as_ptr());
        if tif.is_null() {
            return Err(anyhow!("Failed to open TIFF file: {:?}", path));
        }
        let mut w: u32 = 0;
        let mut h: u32 = 0;
        let mut bps: u16 = 0;
        let mut spp: u16 = 0;
        let mut fmt: u16 = 0;

        TIFFGetField(tif, TIFFTAG_IMAGEWIDTH, &mut w);
        TIFFGetField(tif, TIFFTAG_IMAGELENGTH, &mut h);
        if TIFFGetField(tif, TIFFTAG_BITSPERSAMPLE, &mut bps) == 0 {
            bps = 8;
        }
        if TIFFGetField(tif, TIFFTAG_SAMPLESPERPIXEL, &mut spp) == 0 {
            spp = 1;
        }
        if TIFFGetField(tif, TIFFTAG_SAMPLEFORMAT, &mut fmt) == 0 {
            fmt = SAMPLEFORMAT_UINT;
        }

        TIFFClose(tif);
        Ok((w, h, bps, spp, fmt))
    }
}

#[allow(clippy::too_many_arguments)]
fn run_compression_pass(
    input: &Path,
    output: &Path,
    compression: u16,
    predictor: u16,
    level: Option<u32>,
    quantize: bool,
) -> Result<()> {
    let c_input = CString::new(
        input
            .to_str()
            .ok_or_else(|| anyhow!("Invalid input path"))?,
    )?;

    unsafe {
        let tif_src = TIFFOpen(c_input.as_ptr(), CString::new("r")?.as_ptr());
        if tif_src.is_null() {
            return Err(anyhow!("Failed to open source TIFF"));
        }

        let tmp_path = output.with_extension("tmp_tiffreducer");
        let c_tmp = CString::new(
            tmp_path
                .to_str()
                .ok_or_else(|| anyhow!("Invalid temp path"))?,
        )?;

        let mode_str = if input.metadata()?.len() > 4 * 1024 * 1024 * 1024 {
            "w8"
        } else {
            "w"
        };
        let tif_dst = TIFFOpen(c_tmp.as_ptr(), CString::new(mode_str)?.as_ptr());
        if tif_dst.is_null() {
            TIFFClose(tif_src);
            return Err(anyhow!("Failed to open destination TIFF"));
        }

        // Register GeoTIFF tags on both handles
        crate::metadata::register_geotiff_tags_ffi(tif_src);
        crate::metadata::register_geotiff_tags_ffi(tif_dst);

        let mut page = 0;
        loop {
            process_single_ifd(
                tif_src,
                tif_dst,
                compression,
                predictor,
                level,
                quantize,
                page == 0,
            )?;

            if TIFFReadDirectory(tif_src) == 0 {
                break;
            }
            page += 1;
        }

        TIFFClose(tif_src);
        TIFFClose(tif_dst);

        fs::rename(tmp_path, output)?;
    }
    Ok(())
}

fn run_compression_to_fd(
    input: &Path,
    output_file: std::fs::File,
    compression: u16,
    predictor: u16,
    level: Option<u32>,
    quantize: bool,
) -> Result<u64> {
    let c_input = CString::new(
        input
            .to_str()
            .ok_or_else(|| anyhow!("Invalid input path"))?,
    )?;

    unsafe {
        let tif_src = TIFFOpen(c_input.as_ptr(), CString::new("r")?.as_ptr());
        if tif_src.is_null() {
            return Err(anyhow!("Failed to open source TIFF"));
        }

        let mode_str = if input.metadata()?.len() > 4 * 1024 * 1024 * 1024 {
            "w8"
        } else {
            "w"
        };

        // Use libc::dup to avoid ownership issues with TIFFFdOpen/TIFFClose
        let fd = libc::dup(output_file.as_raw_fd());
        let tif_dst = TIFFFdOpen(
            fd,
            CString::new("dry_run")?.as_ptr(),
            CString::new(mode_str)?.as_ptr(),
        );

        if tif_dst.is_null() {
            libc::close(fd);
            TIFFClose(tif_src);
            return Err(anyhow!("Failed to open destination TIFF (FD)"));
        }

        crate::metadata::register_geotiff_tags_ffi(tif_src);
        crate::metadata::register_geotiff_tags_ffi(tif_dst);

        let mut page = 0;
        loop {
            process_single_ifd(
                tif_src,
                tif_dst,
                compression,
                predictor,
                level,
                quantize,
                page == 0,
            )?;

            if TIFFReadDirectory(tif_src) == 0 {
                break;
            }
            page += 1;
        }

        TIFFClose(tif_src);
        TIFFClose(tif_dst); // This will close the dup'd FD

        use std::io::{Seek, SeekFrom};
        let mut f = output_file;
        let size = f.seek(SeekFrom::End(0))?;
        Ok(size)
    }
}

/// Process a single IFD (Image File Directory) / page
#[allow(clippy::too_many_arguments)]
unsafe fn process_single_ifd(
    tif_src: *mut TIFF,
    tif_dst: *mut TIFF,
    compression: u16,
    requested_predictor: u16,
    level: Option<u32>,
    quantize: bool,
    is_first_page: bool,
) -> Result<()> {
    let mut w = 0u32;
    let mut h = 0u32;
    if TIFFGetField(tif_src, TIFFTAG_IMAGEWIDTH, &mut w) == 0
        || TIFFGetField(tif_src, TIFFTAG_IMAGELENGTH, &mut h) == 0
    {
        return Err(anyhow!("Failed to read image dimensions"));
    }

    // Get source image parameters first
    let mut bps = 0u16;
    let mut spp = 0u16;
    let mut fmt = 0u16;
    let mut photometric: u16 = 0;
    let mut planar: u16 = 0;

    TIFFGetField(tif_src, TIFFTAG_BITSPERSAMPLE, &mut bps);
    TIFFGetField(tif_src, TIFFTAG_SAMPLESPERPIXEL, &mut spp);
    TIFFGetField(tif_src, TIFFTAG_SAMPLEFORMAT, &mut fmt);
    TIFFGetField(tif_src, TIFFTAG_PHOTOMETRIC, &mut photometric);
    TIFFGetField(tif_src, TIFFTAG_PLANARCONFIG, &mut planar);

    if spp == 0 {
        spp = 1;
    }
    if photometric == 0 {
        photometric = PHOTOMETRIC_MINISBLACK;
    }
    if planar == 0 {
        planar = PLANARCONFIG_CONTIG;
    }

    let is_tiled = crate::ffi::TIFFIsTiled(tif_src) != 0;

    TIFFSetField(tif_dst, TIFFTAG_IMAGEWIDTH, w);
    TIFFSetField(tif_dst, TIFFTAG_IMAGELENGTH, h);

    let (target_bps, target_fmt) = if quantize {
        (8u16, SAMPLEFORMAT_UINT)
    } else {
        (bps, fmt)
    };

    TIFFSetField(tif_dst, TIFFTAG_BITSPERSAMPLE, target_bps as u32);
    TIFFSetField(tif_dst, TIFFTAG_SAMPLESPERPIXEL, spp as u32);
    if target_fmt != 0 {
        TIFFSetField(tif_dst, TIFFTAG_SAMPLEFORMAT, target_fmt as u32);
    }
    TIFFSetField(tif_dst, TIFFTAG_PHOTOMETRIC, photometric as u32);
    if planar != 0 && spp > 1 {
        TIFFSetField(tif_dst, TIFFTAG_PLANARCONFIG, planar as u32);
    }

    // Force striped output even if source is tiled (for simplicity and scanline compatibility)
    TIFFSetField(tif_dst, TIFFTAG_ROWSPERSTRIP, h);

    TIFFSetField(tif_dst, TIFFTAG_COMPRESSION, compression as i32);

    let mut xres: f32 = 0.0;
    let mut yres: f32 = 0.0;
    let mut resunit: u16 = 0;
    if TIFFGetField(tif_src, TIFFTAG_XRESOLUTION, &mut xres) != 0 {
        TIFFSetField(tif_dst, TIFFTAG_XRESOLUTION, xres as f64);
    }
    if TIFFGetField(tif_src, TIFFTAG_YRESOLUTION, &mut yres) != 0 {
        TIFFSetField(tif_dst, TIFFTAG_YRESOLUTION, yres as f64);
    }
    if TIFFGetField(tif_src, TIFFTAG_RESOLUTIONUNIT, &mut resunit) != 0 {
        TIFFSetField(tif_dst, TIFFTAG_RESOLUTIONUNIT, resunit as u32);
    }

    if is_first_page {
        clone_metadata(tif_src, tif_dst)?;
    }

    if let Some(lvl) = level {
        match compression {
            COMPRESSION_LZMA => {
                TIFFSetField(tif_dst, TIFFTAG_LZMAPRESET, lvl.clamp(1, 9) as i32);
            }
            COMPRESSION_JPEGXL | COMPRESSION_JPEG | COMPRESSION_WEBP => {
                let tag = match compression {
                    COMPRESSION_JPEGXL => TIFFTAG_DEFLATELEVEL,
                    COMPRESSION_JPEG => TIFFTAG_JPEGQUALITY,
                    COMPRESSION_WEBP => TIFFTAG_WEBP_LEVEL,
                    _ => unreachable!(),
                };
                TIFFSetField(tif_dst, tag, lvl.clamp(1, 100) as i32);
            }
            _ => {}
        }
    }

    let final_predictor = if matches!(
        compression,
        COMPRESSION_LZW
            | COMPRESSION_ADOBE_DEFLATE
            | COMPRESSION_ZSTD
            | COMPRESSION_LZMA
            | COMPRESSION_JPEGXL
    ) {
        match requested_predictor {
            PREDICTOR_HORIZONTAL => {
                if (bps == 8 || bps == 16 || bps == 32)
                    && (fmt == SAMPLEFORMAT_UINT || fmt == SAMPLEFORMAT_INT)
                {
                    PREDICTOR_HORIZONTAL
                } else {
                    PREDICTOR_NONE
                }
            }
            PREDICTOR_FLOATINGPOINT => {
                if fmt == SAMPLEFORMAT_IEEEFP && (bps == 16 || bps == 24 || bps == 32 || bps == 64)
                {
                    PREDICTOR_FLOATINGPOINT
                } else {
                    PREDICTOR_NONE
                }
            }
            _ => PREDICTOR_NONE,
        }
    } else {
        PREDICTOR_NONE
    };

    if final_predictor != PREDICTOR_NONE {
        TIFFSetField(tif_dst, TIFFTAG_PREDICTOR, final_predictor as u32);
    }

    if is_tiled {
        process_tiled_image(tif_src, tif_dst, w, h, spp, bps, fmt, quantize)?;
    } else {
        process_striped_image(tif_src, tif_dst, w, h, spp, bps, fmt, quantize)?;
    }

    TIFFWriteDirectory(tif_dst);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
unsafe fn process_striped_image(
    tif_src: *mut TIFF,
    tif_dst: *mut TIFF,
    w: u32,
    h: u32,
    _spp: u16,
    bps: u16,
    fmt: u16,
    quantize: bool,
) -> Result<()> {
    const MAX_SCANLINE_SIZE: usize = 1024 * 1024 * 1024;
    let in_scanline = TIFFScanlineSize(tif_src) as usize;

    if in_scanline == 0 || in_scanline > MAX_SCANLINE_SIZE {
        return Err(anyhow!("Invalid scanline size: {}", in_scanline));
    }

    let out_scanline = if quantize {
        (w as usize) * (_spp as usize)
    } else {
        in_scanline
    };

    let mut buf_in = vec![0u8; in_scanline];
    let mut buf_out = vec![0u8; out_scanline];

    for row in 0..h {
        if TIFFReadScanline(tif_src, buf_in.as_mut_ptr() as *mut _, row, 0) < 0 {
            return Err(anyhow!("Failed to read scanline {}", row));
        }

        if quantize {
            if bps == 32 && fmt == SAMPLEFORMAT_IEEEFP {
                let slice_f32 = std::slice::from_raw_parts(
                    buf_in.as_ptr() as *const f32,
                    (w * _spp as u32) as usize,
                );
                crate::quantize::quantize_f32_to_u8(slice_f32, &mut buf_out);
            } else if bps == 16 && fmt == SAMPLEFORMAT_INT {
                let slice_i16 = std::slice::from_raw_parts(
                    buf_in.as_ptr() as *const i16,
                    (w * _spp as u32) as usize,
                );
                crate::quantize::quantize_i16_to_u8(slice_i16, &mut buf_out);
            } else if bps == 16 && fmt == SAMPLEFORMAT_UINT {
                let slice_u16 = std::slice::from_raw_parts(
                    buf_in.as_ptr() as *const u16,
                    (w * _spp as u32) as usize,
                );
                crate::quantize::quantize_u16_to_u8(slice_u16, &mut buf_out);
            } else {
                let take = buf_in.len().min(buf_out.len());
                buf_out[..take].copy_from_slice(&buf_in[..take]);
            }
            TIFFWriteScanline(tif_dst, buf_out.as_ptr() as *mut _, row, 0);
        } else {
            TIFFWriteScanline(tif_dst, buf_in.as_ptr() as *mut _, row, 0);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
unsafe fn process_tiled_image(
    tif_src: *mut TIFF,
    tif_dst: *mut TIFF,
    w: u32,
    h: u32,
    spp: u16,
    bps: u16,
    fmt: u16,
    quantize: bool,
) -> Result<()> {
    const MAX_SCANLINE_SIZE: usize = 1024 * 1024 * 1024;

    let mut tile_width: u32 = 0;
    let mut tile_length: u32 = 0;
    TIFFGetField(tif_src, TIFFTAG_TILEWIDTH, &mut tile_width);
    TIFFGetField(tif_src, TIFFTAG_TILELENGTH, &mut tile_length);

    let bytes_per_sample = (bps as usize).div_ceil(8);
    let bytes_per_pixel = bytes_per_sample * (spp as usize);
    let in_row_size = (w as usize) * bytes_per_pixel;

    if in_row_size > MAX_SCANLINE_SIZE {
        return Err(anyhow!("Input row size too large"));
    }

    let out_row_size = if quantize {
        (w as usize) * (spp as usize)
    } else {
        in_row_size
    };

    let image_strip_size = in_row_size * (tile_length as usize);
    let mut image_strip = vec![0u8; image_strip_size];

    let tiles_across = w.div_ceil(tile_width);
    let tiles_down = h.div_ceil(tile_length);

    let tile_buffer_size = (tile_width as usize) * (tile_length as usize) * bytes_per_pixel;
    let mut tile_buf = vec![0u8; tile_buffer_size];

    let mut quant_buf = if quantize {
        vec![0u8; out_row_size]
    } else {
        Vec::new()
    };

    for tile_y in 0..tiles_down {
        image_strip.fill(0);

        for tile_x in 0..tiles_across {
            let tile_index = tile_y * tiles_across + tile_x;
            if crate::ffi::TIFFReadEncodedTile(
                tif_src,
                tile_index,
                tile_buf.as_mut_ptr() as *mut _,
                tile_buffer_size as u32,
            ) < 0
            {
                return Err(anyhow!("Failed to decode tile {}", tile_index));
            }

            let start_x = (tile_x as usize) * (tile_width as usize);
            let actual_width = std::cmp::min(tile_width as usize, w as usize - start_x);
            let actual_height = std::cmp::min(
                tile_length as usize,
                h as usize - (tile_y as usize * tile_length as usize),
            );

            let src_tile_row_size = (tile_width as usize) * bytes_per_pixel;
            for row in 0..actual_height {
                let src_start = row * src_tile_row_size;
                let dst_start = row * in_row_size + (start_x * bytes_per_pixel);
                let copy_len = actual_width * bytes_per_pixel;
                image_strip[dst_start..dst_start + copy_len]
                    .copy_from_slice(&tile_buf[src_start..src_start + copy_len]);
            }
        }

        let rows_in_strip = std::cmp::min(
            tile_length as usize,
            h as usize - (tile_y as usize * tile_length as usize),
        );
        for row_in_strip in 0..rows_in_strip {
            let global_row = tile_y * tile_length + row_in_strip as u32;
            let row_start = row_in_strip * in_row_size;
            let row_slice = &image_strip[row_start..row_start + in_row_size];

            if quantize {
                if bps == 32 && fmt == SAMPLEFORMAT_IEEEFP {
                    let slice_f32 = std::slice::from_raw_parts(
                        row_slice.as_ptr() as *const f32,
                        (w * spp as u32) as usize,
                    );
                    crate::quantize::quantize_f32_to_u8(slice_f32, &mut quant_buf);
                } else if bps == 16 && fmt == SAMPLEFORMAT_INT {
                    let slice_i16 = std::slice::from_raw_parts(
                        row_slice.as_ptr() as *const i16,
                        (w * spp as u32) as usize,
                    );
                    crate::quantize::quantize_i16_to_u8(slice_i16, &mut quant_buf);
                } else if bps == 16 && fmt == SAMPLEFORMAT_UINT {
                    let slice_u16 = std::slice::from_raw_parts(
                        row_slice.as_ptr() as *const u16,
                        (w * spp as u32) as usize,
                    );
                    crate::quantize::quantize_u16_to_u8(slice_u16, &mut quant_buf);
                } else {
                    let take = row_slice.len().min(quant_buf.len());
                    quant_buf[..take].copy_from_slice(&row_slice[..take]);
                }
                TIFFWriteScanline(tif_dst, quant_buf.as_ptr() as *mut _, global_row, 0);
            } else {
                TIFFWriteScanline(tif_dst, row_slice.as_ptr() as *mut _, global_row, 0);
            }
        }
    }
    Ok(())
}
