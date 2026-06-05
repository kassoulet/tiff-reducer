#![allow(clippy::collapsible_if, clippy::redundant_closure_for_method_calls)]

mod ffi;
mod metadata;
mod quantize;
mod wipe;

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

        /// Enable verbose logging for detailed progress
        #[arg(short, long)]
        verbose: bool,
    },
    /// Analyze a TIFF file and display metadata
    Analyze {
        /// Input TIFF file
        #[arg(required = true)]
        path: PathBuf,
    },
    /// Replace image content with synthetic data preserving per-channel
    /// histogram (min/max/mean) while being highly compressible
    Wipe {
        /// Input file(s) or directory
        #[arg(required = true)]
        input: Vec<PathBuf>,

        /// Output file or directory (overwrites input if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Zstd compression level (1-22, default 9)
        #[arg(short, long)]
        level: Option<u32>,

        /// Number of parallel jobs (default: number of CPUs)
        #[arg(short, long)]
        jobs: Option<usize>,

        /// Enable verbose logging for detailed progress
        #[arg(short, long)]
        verbose: bool,
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
    let cli = Cli::parse();

    // Initialize logger based on verbose flag
    let log_level = match &cli.command {
        Commands::Compress { verbose, .. } | Commands::Wipe { verbose, .. } if *verbose => {
            log::LevelFilter::Info
        }
        _ => log::LevelFilter::Warn,
    };

    env_logger::Builder::new()
        .filter_level(log_level)
        .format_target(false)
        .format_timestamp(None)
        .init();

    unsafe {
        suppress_warnings();
    }

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
            verbose,
        } => {
            compress_command(
                input, output, format, level, lossy, quantize, extreme, dry_run, benchmark, jobs,
                verbose,
            )?;
        }
        Commands::Analyze { path } => {
            analyze_command(&path)?;
        }
        Commands::Wipe {
            input,
            output,
            level,
            jobs,
            verbose,
        } => {
            wipe_command(input, output, level, jobs, verbose)?;
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

/// Expand directories to TIFF file lists
fn expand_tiff_inputs(input: &[PathBuf]) -> Result<Vec<PathBuf>> {
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
    Ok(files)
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
    verbose: bool,
) -> Result<()> {
    let files = expand_tiff_inputs(&input)?;

    // With more than one input, --output must be a directory; otherwise every
    // file would resolve to the same target (and temp) path and the parallel
    // workers would race to write/rename it, corrupting the result.
    if files.len() > 1 {
        if let Some(ref out) = output {
            if !out.is_dir() {
                return Err(anyhow!(
                    "Multiple input files require --output to be an existing directory, not a file: {:?}",
                    out
                ));
            }
        }
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
                        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}% {msg}",
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
                verbose,
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
    verbose: bool,
    pb: &ProgressBar,
) -> Result<(u64, u64, String, bool)> {
    let original_size = fs::metadata(input)?.len();
    let start_time = std::time::Instant::now();

    if verbose {
        log::info!("Starting processing of {:?}", input);
    }

    // Get TIFF info to decide on quantization
    let (w, h, bps, spp, sample_format) = get_tiff_info(input)?;
    let is_float = sample_format == SAMPLEFORMAT_IEEEFP;

    // Get IFD count
    let mut total_pages = 0u16;
    unsafe {
        let c_path = CString::new(input.to_str().ok_or_else(|| anyhow!("Invalid path"))?)?;
        let tif = TIFFOpen(c_path.as_ptr(), CString::new("r")?.as_ptr());
        if !tif.is_null() {
            loop {
                total_pages += 1;
                if TIFFReadDirectory(tif) == 0 {
                    break;
                }
            }
            TIFFClose(tif);
        }
    };

    if verbose {
        log::info!(
            "Image dimensions: {}x{}, bps: {}, spp: {}, format: {}, pages: {}",
            w,
            h,
            bps,
            spp,
            sample_format,
            total_pages
        );
    }

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
            if let Ok(size) = run_compression_to_fd(
                input,
                temp_file,
                cid,
                pid,
                effective_level,
                quantize,
                verbose,
                total_pages,
                pb,
            ) {
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

        let dry_run_size = run_compression_to_fd(
            input,
            temp_file,
            cid,
            pid,
            effective_level,
            quantize,
            verbose,
            total_pages,
            pb,
        )?;

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
    run_compression_pass(
        input,
        output,
        cid,
        pid,
        effective_level,
        quantize,
        verbose,
        total_pages,
        pb,
    )?;

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
    verbose: bool,
    total_pages: u16,
    pb: &ProgressBar,
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
            if verbose {
                log::info!("Processing IFD {}", page);
            }
            pb.set_message(format!("Page {}/{}", page + 1, total_pages));
            pb.set_position(((page as u64) * 100) / (total_pages as u64));

            process_single_ifd(
                input,
                tif_src,
                tif_dst,
                compression,
                predictor,
                level,
                quantize,
                page == 0,
                verbose,
                page,
                total_pages,
                pb,
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

#[allow(clippy::too_many_arguments)]
fn run_compression_to_fd(
    input: &Path,
    output_file: std::fs::File,
    compression: u16,
    predictor: u16,
    level: Option<u32>,
    quantize: bool,
    verbose: bool,
    total_pages: u16,
    pb: &ProgressBar,
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
            if verbose {
                log::info!("Processing IFD {} (dry-run)", page);
            }
            pb.set_message(format!("Page {}/{} (dry-run)", page + 1, total_pages));
            pb.set_position(((page as u64) * 100) / (total_pages as u64));

            process_single_ifd(
                input,
                tif_src,
                tif_dst,
                compression,
                predictor,
                level,
                quantize,
                page == 0,
                verbose,
                page,
                total_pages,
                pb,
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
    input_path: &Path, // Need path to open more handles for tiled processing
    tif_src: *mut TIFF,
    tif_dst: *mut TIFF,
    compression: u16,
    requested_predictor: u16,
    level: Option<u32>,
    quantize: bool,
    _is_first_page: bool,
    verbose: bool,
    page_index: u16,
    total_pages: u16,
    pb: &ProgressBar,
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

    if photometric == PHOTOMETRIC_YCBCR {
        let mut h_sub: u16 = 0;
        let mut v_sub: u16 = 0;
        if TIFFGetField(tif_src, TIFFTAG_YCBCRSUBSAMPLING, &mut h_sub, &mut v_sub) != 0 {
            if h_sub != 1 || v_sub != 1 {
                return Err(anyhow!(
                    "YCbCr subsampling ({},{}) is not supported and causes crashes",
                    h_sub,
                    v_sub
                ));
            }
        }
    }

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

    // Force striped output even if source is tiled
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

    clone_metadata(tif_src, tif_dst)?;

    if let Some(lvl) = level {
        match compression {
            COMPRESSION_LZMA => {
                TIFFSetField(tif_dst, TIFFTAG_LZMAPRESET, lvl.clamp(1, 9) as i32);
            }
            COMPRESSION_ZSTD => {
                let clamped: i32 = lvl.clamp(1, 22) as i32;
                TIFFSetField(tif_dst, TIFFTAG_ZSTD_LEVEL, clamped);
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
        if verbose {
            pb.println("Image is tiled, using parallel tiled processing path");
        }
        process_tiled_image(
            input_path,
            tif_src,
            tif_dst,
            w,
            h,
            spp,
            bps,
            fmt,
            planar,
            quantize,
            verbose,
            page_index,
            total_pages,
            pb,
        )?;
    } else {
        if verbose {
            pb.println("Image is striped, using striped processing path");
        }
        process_striped_image(
            tif_src, tif_dst, w, h, spp, bps, fmt, planar, quantize, verbose, pb,
        )?;
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
    spp: u16,
    bps: u16,
    fmt: u16,
    planar: u16,
    quantize: bool,
    verbose: bool,
    pb: &ProgressBar,
) -> Result<()> {
    const MAX_SCANLINE_SIZE: usize = 1024 * 1024 * 1024;
    let in_scanline = TIFFScanlineSize(tif_src) as usize;

    if in_scanline == 0 || in_scanline > MAX_SCANLINE_SIZE {
        return Err(anyhow!("Invalid scanline size: {}", in_scanline));
    }

    let num_samples = if planar == PLANARCONFIG_SEPARATE {
        spp
    } else {
        1
    };

    let out_row_size = if quantize {
        (w as usize)
            * (if planar == PLANARCONFIG_SEPARATE {
                1
            } else {
                spp as usize
            })
    } else {
        in_scanline
    };

    let mut buf_in = vec![0u8; in_scanline];
    let mut buf_out = vec![0u8; out_row_size];

    for s in 0..num_samples {
        for row in 0..h {
            if verbose && row % 1000 == 0 {
                if num_samples > 1 {
                    pb.println(format!(
                        "Processing scanline {}/{} (sample {}/{})",
                        row,
                        h,
                        s + 1,
                        num_samples
                    ));
                } else {
                    pb.println(format!("Processing scanline {}/{}", row, h));
                }
            }

            if TIFFReadScanline(tif_src, buf_in.as_mut_ptr() as *mut _, row, s) < 0 {
                return Err(anyhow!("Failed to read scanline {} sample {}", row, s));
            }

            if quantize {
                let spp_eff = if planar == PLANARCONFIG_SEPARATE {
                    1
                } else {
                    spp as u32
                };

                if bps == 32 && fmt == SAMPLEFORMAT_IEEEFP {
                    let slice_f32 = std::slice::from_raw_parts(
                        buf_in.as_ptr() as *const f32,
                        (w * spp_eff) as usize,
                    );
                    crate::quantize::quantize_f32_to_u8(slice_f32, &mut buf_out);
                } else if bps == 64 && fmt == SAMPLEFORMAT_IEEEFP {
                    let slice_f64 = std::slice::from_raw_parts(
                        buf_in.as_ptr() as *const f64,
                        (w * spp_eff) as usize,
                    );
                    crate::quantize::quantize_f64_to_u8(slice_f64, &mut buf_out);
                } else if bps == 16 && fmt == SAMPLEFORMAT_INT {
                    let slice_i16 = std::slice::from_raw_parts(
                        buf_in.as_ptr() as *const i16,
                        (w * spp_eff) as usize,
                    );
                    crate::quantize::quantize_i16_to_u8(slice_i16, &mut buf_out);
                } else if bps == 16 && fmt == SAMPLEFORMAT_UINT {
                    let slice_u16 = std::slice::from_raw_parts(
                        buf_in.as_ptr() as *const u16,
                        (w * spp_eff) as usize,
                    );
                    crate::quantize::quantize_u16_to_u8(slice_u16, &mut buf_out);
                } else {
                    let take = buf_in.len().min(buf_out.len());
                    buf_out[..take].copy_from_slice(&buf_in[..take]);
                }
                TIFFWriteScanline(tif_dst, buf_out.as_ptr() as *mut _, row, s);
            } else {
                TIFFWriteScanline(tif_dst, buf_in.as_ptr() as *mut _, row, s);
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
unsafe fn process_tiled_image(
    input_path: &Path, // Need path to open more handles
    tif_src: *mut TIFF,
    tif_dst: *mut TIFF,
    w: u32,
    h: u32,
    spp: u16,
    bps: u16,
    fmt: u16,
    planar: u16,
    quantize: bool,
    verbose: bool,
    page_index: u16,
    total_pages: u16,
    pb: &ProgressBar,
) -> Result<()> {
    const MAX_SCANLINE_SIZE: usize = 1024 * 1024 * 1024;

    let mut tile_width: u32 = 0;
    let mut tile_length: u32 = 0;
    TIFFGetField(tif_src, TIFFTAG_TILEWIDTH, &mut tile_width);
    TIFFGetField(tif_src, TIFFTAG_TILELENGTH, &mut tile_length);

    if verbose {
        pb.println(format!("Tile dimensions: {}x{}", tile_width, tile_length));
    }

    let bytes_per_sample = (bps as usize).div_ceil(8);
    let bytes_per_pixel = bytes_per_sample
        * (if planar == PLANARCONFIG_SEPARATE {
            1
        } else {
            spp as usize
        });
    let in_row_size = (w as usize) * bytes_per_pixel;

    if in_row_size > MAX_SCANLINE_SIZE {
        return Err(anyhow!("Input row size too large"));
    }

    let out_row_size = if quantize {
        (w as usize)
            * (if planar == PLANARCONFIG_SEPARATE {
                1
            } else {
                spp as usize
            })
    } else {
        in_row_size
    };

    let image_strip_size = in_row_size * (tile_length as usize);
    let mut image_strip = vec![0u8; image_strip_size];

    let tiles_across = w.div_ceil(tile_width);
    let tiles_down = h.div_ceil(tile_length);
    let tiles_per_plane = tiles_across * tiles_down;

    let tile_buffer_size = (tile_width as usize) * (tile_length as usize) * bytes_per_pixel;

    // Use a thread-local TIFF handle for parallel decompression
    let c_path = CString::new(input_path.to_str().unwrap())?;
    thread_local! {
        static SOURCE_HANDLES: std::cell::RefCell<Option<*mut TIFF>> = const { std::cell::RefCell::new(None) };
    }

    let num_samples = if planar == PLANARCONFIG_SEPARATE {
        spp
    } else {
        1
    };

    for s in 0..num_samples {
        for tile_y in 0..tiles_down {
            if verbose {
                if num_samples > 1 {
                    pb.println(format!(
                        "Processing tile row {}/{} (sample {}/{})",
                        tile_y,
                        tiles_down,
                        s + 1,
                        num_samples
                    ));
                } else {
                    pb.println(format!("Processing tile row {}/{}", tile_y, tiles_down));
                }
            }

            pb.set_message(format!(
                "(IFD {}/{} Tile row {}/{})",
                page_index + 1,
                total_pages,
                tile_y + 1,
                tiles_down
            ));
            pb.set_position(
                ((page_index as u64) * 100 + (tile_y as u64 * 100 / tiles_down as u64))
                    / (total_pages as u64),
            );

            image_strip.fill(0);

            // Prepare tile metadata for parallel decoding
            let tile_indices: Vec<(u32, u32)> = (0..tiles_across)
                .map(|tile_x| {
                    let tile_in_plane = tile_y * tiles_across + tile_x;
                    let tile_index = if planar == PLANARCONFIG_SEPARATE {
                        (s as u32 * tiles_per_plane) + tile_in_plane
                    } else {
                        tile_in_plane
                    };
                    (tile_x, tile_index)
                })
                .collect();

            // Parallel decode tiles
            let decoded_tiles: Vec<(u32, Vec<u8>)> = tile_indices
                .par_iter()
                .map(|&(tile_x, tile_index)| {
                    let mut buf = vec![0u8; tile_buffer_size];
                    SOURCE_HANDLES.with(|cell| {
                        let mut cell = cell.borrow_mut();
                        if cell.is_none() {
                            let tif = unsafe {
                                TIFFOpen(c_path.as_ptr(), CString::new("r").unwrap().as_ptr())
                            };
                            // We must re-register tags for every handle
                            unsafe { crate::metadata::register_geotiff_tags_ffi(tif) };
                            *cell = Some(tif);
                        }
                        let tif = cell.unwrap();
                        unsafe {
                            if crate::ffi::TIFFReadEncodedTile(
                                tif,
                                tile_index,
                                buf.as_mut_ptr() as *mut _,
                                tile_buffer_size as u32,
                            ) < 0
                            {
                                // Should ideally return an error, but par_iter map needs a return
                            }
                        }
                    });
                    (tile_x, buf)
                })
                .collect();

            // Assembly (Sequential)
            for (tile_x, tile_buf) in decoded_tiles {
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

            // Parallelize quantization of rows in the strip
            let mut processed_rows = vec![vec![0u8; out_row_size]; rows_in_strip];

            processed_rows
                .par_iter_mut()
                .enumerate()
                .for_each(|(row_idx, out_buf)| {
                    let row_start = row_idx * in_row_size;
                    let row_slice = &image_strip[row_start..row_start + in_row_size];

                    if quantize {
                        let spp_eff = if planar == PLANARCONFIG_SEPARATE {
                            1
                        } else {
                            spp as u32
                        };

                        if bps == 32 && fmt == SAMPLEFORMAT_IEEEFP {
                            let slice_f32 = unsafe {
                                std::slice::from_raw_parts(
                                    row_slice.as_ptr() as *const f32,
                                    (w * spp_eff) as usize,
                                )
                            };
                            crate::quantize::quantize_f32_to_u8(slice_f32, out_buf);
                        } else if bps == 64 && fmt == SAMPLEFORMAT_IEEEFP {
                            let slice_f64 = unsafe {
                                std::slice::from_raw_parts(
                                    row_slice.as_ptr() as *const f64,
                                    (w * spp_eff) as usize,
                                )
                            };
                            crate::quantize::quantize_f64_to_u8(slice_f64, out_buf);
                        } else if bps == 16 && fmt == SAMPLEFORMAT_INT {
                            let slice_i16 = unsafe {
                                std::slice::from_raw_parts(
                                    row_slice.as_ptr() as *const i16,
                                    (w * spp_eff) as usize,
                                )
                            };
                            crate::quantize::quantize_i16_to_u8(slice_i16, out_buf);
                        } else if bps == 16 && fmt == SAMPLEFORMAT_UINT {
                            let slice_u16 = unsafe {
                                std::slice::from_raw_parts(
                                    row_slice.as_ptr() as *const u16,
                                    (w * spp_eff) as usize,
                                )
                            };
                            crate::quantize::quantize_u16_to_u8(slice_u16, out_buf);
                        } else {
                            let take = row_slice.len().min(out_buf.len());
                            out_buf[..take].copy_from_slice(&row_slice[..take]);
                        }
                    }
                });

            // Sequential write
            for (row_idx, out_buf) in processed_rows.iter().enumerate().take(rows_in_strip) {
                let global_row = tile_y * tile_length + row_idx as u32;
                if quantize {
                    TIFFWriteScanline(tif_dst, out_buf.as_ptr() as *mut _, global_row, s);
                } else {
                    let row_start = row_idx * in_row_size;
                    let row_slice = &image_strip[row_start..row_start + in_row_size];
                    TIFFWriteScanline(tif_dst, row_slice.as_ptr() as *mut _, global_row, s);
                }
            }
        }
    }
    Ok(())
}

fn wipe_command(
    input: Vec<PathBuf>,
    output: Option<PathBuf>,
    level: Option<u32>,
    jobs: Option<usize>,
    verbose: bool,
) -> Result<()> {
    let files = expand_tiff_inputs(&input)?;

    // With more than one input, --output must be a directory; otherwise every
    // file would resolve to the same target (and temp) path and the parallel
    // workers would race to write/rename it, corrupting the result.
    if files.len() > 1 {
        if let Some(ref out) = output {
            if !out.is_dir() {
                return Err(anyhow!(
                    "Multiple input files require --output to be an existing directory, not a file: {:?}",
                    out
                ));
            }
        }
    }

    let m = MultiProgress::new();
    let num_jobs = jobs.unwrap_or_else(num_cpus::get);

    files
        .par_iter()
        .with_max_len(num_jobs)
        .for_each(|file_path| {
            let pb = m.add(ProgressBar::new(100));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}% {msg}",
                    )
                    .unwrap(),
            );
            pb.set_position(0);
            pb.set_message(format!(
                "Wiping {:?}",
                file_path.file_name().unwrap_or(file_path.as_os_str())
            ));

            let target_output = if let Some(ref out) = output {
                if out.is_dir() {
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

            match wipe_single_file(file_path, &target_output, level, verbose, &pb) {
                Ok((original, wiped)) => {
                    pb.finish();
                    let ratio = if original > 0 {
                        (1.0 - (wiped as f64 / original as f64)) * 100.0
                    } else {
                        0.0
                    };
                    println!(
                        "\n[{}] Wiped: {} -> {} bytes ({:.1}% reduction)",
                        file_path
                            .file_name()
                            .unwrap_or(file_path.as_os_str())
                            .to_string_lossy(),
                        original,
                        wiped,
                        ratio
                    );
                }
                Err(e) => {
                    pb.finish_with_message(format!("Error: {}", e));
                }
            }
        });

    Ok(())
}

fn wipe_single_file(
    input: &Path,
    output: &Path,
    level: Option<u32>,
    verbose: bool,
    pb: &ProgressBar,
) -> Result<(u64, u64)> {
    let original_size = fs::metadata(input)?.len();

    // Get IFD count
    let mut total_pages = 0u16;
    unsafe {
        let c_path = CString::new(input.to_str().ok_or_else(|| anyhow!("Invalid path"))?)?;
        let tif = TIFFOpen(c_path.as_ptr(), CString::new("r")?.as_ptr());
        if tif.is_null() {
            return Err(anyhow!("Failed to open TIFF file: {:?}", input));
        }
        loop {
            total_pages += 1;
            if TIFFReadDirectory(tif) == 0 {
                break;
            }
        }
        TIFFClose(tif);
    };

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
            if verbose {
                log::info!("Wiping IFD {}", page);
            }
            pb.set_message(format!("Page {}/{}", page + 1, total_pages));
            pb.set_position(((page as u64) * 100) / (total_pages as u64));

            let result = wipe_single_ifd(input, tif_src, tif_dst, level, page, verbose, pb);
            if let Err(e) = result {
                TIFFClose(tif_src);
                TIFFClose(tif_dst);
                let _ = fs::remove_file(&tmp_path);
                return Err(e);
            }

            if TIFFReadDirectory(tif_src) == 0 {
                break;
            }
            page += 1;
        }

        TIFFClose(tif_src);
        TIFFClose(tif_dst);

        fs::rename(tmp_path, output)?;
    }

    let wiped_size = fs::metadata(output)?.len();
    Ok((original_size, wiped_size))
}

/// Wipe a single IFD: clone structure and metadata, but replace pixel data
/// with per-channel sorted values (same histogram, highly compressible).
///
/// Two strategies (see `src/wipe.rs`):
/// - 8/16-bit integers: histogram streaming — O(1) memory, no sort, parallel
///   tile decode.
/// - everything else: read the plane into memory and parallel-sort it.
#[allow(clippy::too_many_arguments)]
unsafe fn wipe_single_ifd(
    input_path: &Path,
    tif_src: *mut TIFF,
    tif_dst: *mut TIFF,
    level: Option<u32>,
    page: u16,
    verbose: bool,
    pb: &ProgressBar,
) -> Result<()> {
    let mut w = 0u32;
    let mut h = 0u32;
    if TIFFGetField(tif_src, TIFFTAG_IMAGEWIDTH, &mut w) == 0
        || TIFFGetField(tif_src, TIFFTAG_IMAGELENGTH, &mut h) == 0
    {
        return Err(anyhow!("Failed to read image dimensions"));
    }

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

    if photometric == PHOTOMETRIC_YCBCR {
        let mut h_sub: u16 = 0;
        let mut v_sub: u16 = 0;
        if TIFFGetField(tif_src, TIFFTAG_YCBCRSUBSAMPLING, &mut h_sub, &mut v_sub) != 0 {
            if h_sub != 1 || v_sub != 1 {
                return Err(anyhow!(
                    "YCbCr subsampling ({},{}) is not supported and causes crashes",
                    h_sub,
                    v_sub
                ));
            }
        }
    }

    if bps == 0 {
        bps = 8;
    }
    if spp == 0 {
        spp = 1;
    }
    if fmt == 0 {
        fmt = SAMPLEFORMAT_UINT;
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
    TIFFSetField(tif_dst, TIFFTAG_BITSPERSAMPLE, bps as u32);
    TIFFSetField(tif_dst, TIFFTAG_SAMPLESPERPIXEL, spp as u32);
    TIFFSetField(tif_dst, TIFFTAG_SAMPLEFORMAT, fmt as u32);
    TIFFSetField(tif_dst, TIFFTAG_PHOTOMETRIC, photometric as u32);
    if spp > 1 {
        TIFFSetField(tif_dst, TIFFTAG_PLANARCONFIG, planar as u32);
    }

    // Force striped output even if source is tiled
    TIFFSetField(tif_dst, TIFFTAG_ROWSPERSTRIP, h);

    TIFFSetField(tif_dst, TIFFTAG_COMPRESSION, COMPRESSION_ZSTD as i32);
    // Sorted data is near-RLE: high zstd levels barely shrink it further but
    // cost a lot of encode time, so default lower than compress does
    let zstd_level: i32 = level.unwrap_or(9).clamp(1, 22) as i32;
    TIFFSetField(tif_dst, TIFFTAG_ZSTD_LEVEL, zstd_level);

    // Sorted data is monotonic: a predictor turns it into near-constant deltas
    let predictor = if fmt == SAMPLEFORMAT_IEEEFP && matches!(bps, 16 | 24 | 32 | 64) {
        PREDICTOR_FLOATINGPOINT
    } else if matches!(bps, 8 | 16 | 32) && (fmt == SAMPLEFORMAT_UINT || fmt == SAMPLEFORMAT_INT) {
        PREDICTOR_HORIZONTAL
    } else {
        PREDICTOR_NONE
    };
    if predictor != PREDICTOR_NONE {
        TIFFSetField(tif_dst, TIFFTAG_PREDICTOR, predictor as u32);
    }

    clone_metadata(tif_src, tif_dst)?;

    // Channels interleaved within one plane buffer
    let interleaved_spp = if planar == PLANARCONFIG_SEPARATE {
        1usize
    } else {
        spp as usize
    };
    let num_planes = if planar == PLANARCONFIG_SEPARATE {
        spp
    } else {
        1
    };

    // Sample widths >= 8 that are not a whole number of bytes (e.g. 12-bit) are
    // packed tightly by libtiff, but the read/sort path below assumes a
    // whole-byte sample stride, so the data would be silently mis-unpacked and
    // the histogram corrupted. Reject rather than produce wrong output.
    if bps > 8 && bps % 8 != 0 {
        return Err(anyhow!(
            "{}-bit samples (not a multiple of 8) are not supported for wipe",
            bps
        ));
    }

    // Sub-byte (1/2/4-bit) data is wiped by sorting whole bytes. That only
    // preserves the per-sample histogram when each byte holds whole samples of
    // a single channel and rows carry no padding bits (i.e. the row is an exact
    // number of bytes). Otherwise a byte-level sort mixes padding bits or
    // channels into the counts, silently violating the preservation guarantee.
    if bps < 8 && (interleaved_spp > 1 || ((w as usize) * (bps as usize)) % 8 != 0) {
        return Err(anyhow!(
            "Sub-byte images ({}-bit, {} interleaved channel(s), width {}) cannot be \
             wiped while preserving the per-channel histogram",
            bps,
            interleaved_spp,
            w
        ));
    }

    let bytes_per_sample = (bps as usize).div_ceil(8);
    let in_row_size = if bps >= 8 {
        (w as usize) * bytes_per_sample * interleaved_spp
    } else {
        // Packed sub-byte data: rows are padded to byte boundary
        ((w as usize) * (bps as usize) * interleaved_spp).div_ceil(8)
    };

    let use_histogram = bps >= 8 && crate::wipe::Histogram::supports(bps, fmt);

    for s in 0..num_planes {
        if verbose {
            log::info!(
                "Wiping plane {}/{} ({})",
                s + 1,
                num_planes,
                if use_histogram { "histogram" } else { "sort" }
            );
        }

        if use_histogram {
            // Pass 1: accumulate per-channel histograms only (O(1) memory)
            pb.set_message(format!("Reading plane {}/{}", s + 1, num_planes));
            let hist = if is_tiled {
                histogram_tiled_plane(
                    input_path,
                    tif_src,
                    w,
                    h,
                    interleaved_spp,
                    bytes_per_sample,
                    s,
                    num_planes,
                    page,
                    bps,
                    fmt,
                )?
            } else {
                let in_scanline = TIFFScanlineSize(tif_src) as usize;
                if in_scanline == 0 || in_scanline > in_row_size {
                    return Err(anyhow!("Invalid scanline size: {}", in_scanline));
                }
                let mut hist = crate::wipe::Histogram::new(interleaved_spp, bps, fmt);
                let mut row_buf = vec![0u8; in_row_size];
                for row in 0..h {
                    if TIFFReadScanline(tif_src, row_buf.as_mut_ptr() as *mut _, row, s) < 0 {
                        return Err(anyhow!("Failed to read scanline {} sample {}", row, s));
                    }
                    hist.accumulate(&row_buf);
                }
                hist
            };

            // Pass 2: synthesize the sorted rows directly from the histogram
            pb.set_message(format!("Writing plane {}/{}", s + 1, num_planes));
            let mut synth = hist.synthesizer();
            let mut row_buf = vec![0u8; in_row_size];
            for row in 0..h {
                synth.synthesize_row(&mut row_buf);
                if TIFFWriteScanline(tif_dst, row_buf.as_ptr() as *mut _, row, s) < 0 {
                    return Err(anyhow!("Failed to write scanline {} sample {}", row, s));
                }
            }
        } else {
            // Fallback: read the whole plane and sort it in memory
            const MAX_PLANE_SIZE: usize = 16 * 1024 * 1024 * 1024;
            let plane_size = in_row_size
                .checked_mul(h as usize)
                .filter(|&sz| sz > 0 && sz <= MAX_PLANE_SIZE)
                .ok_or_else(|| anyhow!("Image plane too large to wipe in memory"))?;

            let mut plane = vec![0u8; plane_size];

            if is_tiled {
                read_tiled_plane(
                    input_path,
                    tif_src,
                    &mut plane,
                    w,
                    h,
                    in_row_size,
                    s,
                    num_planes,
                    page,
                )?;
            } else {
                let in_scanline = TIFFScanlineSize(tif_src) as usize;
                if in_scanline == 0 || in_scanline > in_row_size {
                    return Err(anyhow!("Invalid scanline size: {}", in_scanline));
                }
                for row in 0..h {
                    let offset = (row as usize) * in_row_size;
                    if TIFFReadScanline(tif_src, plane[offset..].as_mut_ptr() as *mut _, row, s) < 0
                    {
                        return Err(anyhow!("Failed to read scanline {} sample {}", row, s));
                    }
                }
            }

            pb.set_message(format!("Sorting plane {}/{}", s + 1, num_planes));
            crate::wipe::wipe_buffer(&mut plane, interleaved_spp, bps, fmt);

            for row in 0..h {
                let offset = (row as usize) * in_row_size;
                if TIFFWriteScanline(tif_dst, plane[offset..].as_ptr() as *mut _, row, s) < 0 {
                    return Err(anyhow!("Failed to write scanline {} sample {}", row, s));
                }
            }
        }
    }

    TIFFWriteDirectory(tif_dst);
    Ok(())
}

/// One tile's coordinates and valid (non-padding) region
struct TileJob {
    index: u32,
    actual_width: usize,
    actual_height: usize,
}

/// Build the tile job list for one plane, reading tile dimensions from the
/// source handle. Returns (jobs, tile_width, tile_length).
unsafe fn tile_jobs(
    tif_src: *mut TIFF,
    w: u32,
    h: u32,
    sample: u16,
    num_planes: u16,
) -> Result<(Vec<TileJob>, u32, u32)> {
    let mut tile_width: u32 = 0;
    let mut tile_length: u32 = 0;
    TIFFGetField(tif_src, TIFFTAG_TILEWIDTH, &mut tile_width);
    TIFFGetField(tif_src, TIFFTAG_TILELENGTH, &mut tile_length);
    if tile_width == 0 || tile_length == 0 {
        return Err(anyhow!("Invalid tile dimensions"));
    }

    let tiles_across = w.div_ceil(tile_width);
    let tiles_down = h.div_ceil(tile_length);
    let tiles_per_plane = tiles_across * tiles_down;

    let mut jobs = Vec::with_capacity((tiles_per_plane) as usize);
    for tile_y in 0..tiles_down {
        for tile_x in 0..tiles_across {
            let tile_in_plane = tile_y * tiles_across + tile_x;
            let index = if num_planes > 1 {
                (sample as u32 * tiles_per_plane) + tile_in_plane
            } else {
                tile_in_plane
            };
            let start_x = (tile_x as usize) * (tile_width as usize);
            let start_y = (tile_y as usize) * (tile_length as usize);
            jobs.push(TileJob {
                index,
                actual_width: std::cmp::min(tile_width as usize, w as usize - start_x),
                actual_height: std::cmp::min(tile_length as usize, h as usize - start_y),
            });
        }
    }
    Ok((jobs, tile_width, tile_length))
}

/// Open an independent read handle on the source file, positioned at `page`.
/// Used by parallel tile workers (each worker gets its own handle, so there
/// is no cross-thread or cross-file state).
unsafe fn open_worker_handle(c_path: &CString, page: u16) -> Result<*mut TIFF> {
    let tif = TIFFOpen(c_path.as_ptr(), CString::new("r")?.as_ptr());
    if tif.is_null() {
        return Err(anyhow!("Failed to open source TIFF (worker)"));
    }
    crate::metadata::register_geotiff_tags_ffi(tif);
    if page > 0 && TIFFSetDirectory(tif, page) == 0 {
        TIFFClose(tif);
        return Err(anyhow!("Failed to set directory {} (worker)", page));
    }
    Ok(tif)
}

/// Accumulate per-channel histograms of one plane of a tiled image,
/// decoding tiles in parallel. Tile padding (beyond the image edge) is
/// excluded from the counts.
#[allow(clippy::too_many_arguments)]
unsafe fn histogram_tiled_plane(
    input_path: &Path,
    tif_src: *mut TIFF,
    w: u32,
    h: u32,
    interleaved_spp: usize,
    bytes_per_sample: usize,
    sample: u16,
    num_planes: u16,
    page: u16,
    bps: u16,
    fmt: u16,
) -> Result<crate::wipe::Histogram> {
    let (jobs, tile_width, tile_length) = tile_jobs(tif_src, w, h, sample, num_planes)?;

    let bytes_per_pixel = bytes_per_sample * interleaved_spp;
    let tile_buffer_size = (tile_width as usize) * (tile_length as usize) * bytes_per_pixel;
    let src_tile_row_size = (tile_width as usize) * bytes_per_pixel;

    let c_path = CString::new(
        input_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid input path"))?,
    )?;

    // Each task decodes a chunk of tiles with its own handle and merges a
    // local histogram; chunking amortizes the open/close cost.
    const TILES_PER_TASK: usize = 32;
    jobs.par_chunks(TILES_PER_TASK)
        .map(|chunk| -> Result<crate::wipe::Histogram> {
            let mut hist = crate::wipe::Histogram::new(interleaved_spp, bps, fmt);
            let tif = unsafe { open_worker_handle(&c_path, page)? };
            let mut tile_buf = vec![0u8; tile_buffer_size];
            for job in chunk {
                let read = unsafe {
                    crate::ffi::TIFFReadEncodedTile(
                        tif,
                        job.index,
                        tile_buf.as_mut_ptr() as *mut _,
                        tile_buffer_size as u32,
                    )
                };
                if read < 0 {
                    unsafe { TIFFClose(tif) };
                    return Err(anyhow!("Failed to read tile {}", job.index));
                }
                let valid_row = job.actual_width * bytes_per_pixel;
                for row in 0..job.actual_height {
                    let start = row * src_tile_row_size;
                    hist.accumulate(&tile_buf[start..start + valid_row]);
                }
            }
            unsafe { TIFFClose(tif) };
            Ok(hist)
        })
        .try_reduce(
            || crate::wipe::Histogram::new(interleaved_spp, bps, fmt),
            |a, b| Ok(a.merge(b)),
        )
}

/// Read one full plane of a tiled image into a row-major buffer, decoding
/// one band (horizontal row of tiles) per parallel task. Bands map to
/// disjoint chunks of the plane, so workers never overlap.
#[allow(clippy::too_many_arguments)]
unsafe fn read_tiled_plane(
    input_path: &Path,
    tif_src: *mut TIFF,
    plane: &mut [u8],
    w: u32,
    h: u32,
    in_row_size: usize,
    sample: u16,
    num_planes: u16,
    page: u16,
) -> Result<()> {
    let mut tile_width: u32 = 0;
    let mut tile_length: u32 = 0;
    TIFFGetField(tif_src, TIFFTAG_TILEWIDTH, &mut tile_width);
    TIFFGetField(tif_src, TIFFTAG_TILELENGTH, &mut tile_length);
    if tile_width == 0 || tile_length == 0 {
        return Err(anyhow!("Invalid tile dimensions"));
    }

    let bytes_per_pixel = in_row_size / (w as usize);
    if bytes_per_pixel == 0 {
        return Err(anyhow!("Sub-byte tiled images are not supported for wipe"));
    }
    let tiles_across = w.div_ceil(tile_width);
    let tiles_down = h.div_ceil(tile_length);
    let tiles_per_plane = tiles_across * tiles_down;

    let tile_buffer_size = (tile_width as usize) * (tile_length as usize) * bytes_per_pixel;
    let src_tile_row_size = (tile_width as usize) * bytes_per_pixel;
    let band_size = in_row_size * (tile_length as usize);

    let c_path = CString::new(
        input_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid input path"))?,
    )?;

    plane
        .par_chunks_mut(band_size)
        .enumerate()
        .map(|(tile_y, band)| -> Result<()> {
            let tif = unsafe { open_worker_handle(&c_path, page)? };
            let mut tile_buf = vec![0u8; tile_buffer_size];

            let start_y = tile_y * (tile_length as usize);
            let band_rows = std::cmp::min(tile_length as usize, h as usize - start_y);

            for tile_x in 0..tiles_across {
                let tile_in_plane = (tile_y as u32) * tiles_across + tile_x;
                let tile_index = if num_planes > 1 {
                    (sample as u32 * tiles_per_plane) + tile_in_plane
                } else {
                    tile_in_plane
                };

                let read = unsafe {
                    crate::ffi::TIFFReadEncodedTile(
                        tif,
                        tile_index,
                        tile_buf.as_mut_ptr() as *mut _,
                        tile_buffer_size as u32,
                    )
                };
                if read < 0 {
                    unsafe { TIFFClose(tif) };
                    return Err(anyhow!("Failed to read tile {}", tile_index));
                }

                let start_x = (tile_x as usize) * (tile_width as usize);
                let actual_width = std::cmp::min(tile_width as usize, w as usize - start_x);

                for row in 0..band_rows {
                    let src_start = row * src_tile_row_size;
                    let dst_start = row * in_row_size + start_x * bytes_per_pixel;
                    let copy_len = actual_width * bytes_per_pixel;
                    band[dst_start..dst_start + copy_len]
                        .copy_from_slice(&tile_buf[src_start..src_start + copy_len]);
                }
            }
            unsafe { TIFFClose(tif) };
            Ok(())
        })
        .collect::<Result<Vec<()>>>()?;
    Ok(())
}
