#![allow(clippy::collapsible_if, clippy::redundant_closure_for_method_calls)]

mod ffi;
mod metadata;
mod quantize;
mod wipe;

use crate::ffi::*;
use crate::metadata::clone_metadata;
use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::ffi::CString;
use std::fs;
use std::hash::Hasher;
use std::os::unix::io::AsRawFd;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Create a per-file progress bar with the shared bar style and initial message.
fn new_file_progress(m: &MultiProgress, message: String) -> ProgressBar {
    let pb = m.add(ProgressBar::new(100));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}% {msg}")
            .unwrap(),
    );
    pb.set_position(0);
    pb.set_message(message);
    pb
}

/// Whether `--output` names a directory: an existing one, or a path ending in
/// a separator (created when the first file is written).
fn is_output_dir(out: &Path) -> bool {
    out.is_dir()
        || out
            .as_os_str()
            .as_encoded_bytes()
            .last()
            .is_some_and(|&b| std::path::is_separator(b as char))
}

/// With more than one input, --output must be a directory; otherwise every
/// file would resolve to the same target (and temp) path and the parallel
/// workers would race to write/rename it, corrupting the result.
fn check_batch_output(file_count: usize, output: &Option<PathBuf>) -> Result<()> {
    match output {
        Some(out) if file_count > 1 && !is_output_dir(out) => Err(anyhow!(
            "Multiple input files require --output to be a directory (existing, or ending in '/'), not a file: {:?}",
            out
        )),
        _ => Ok(()),
    }
}

/// Resolve the destination path for one input file given the optional
/// `--output`. With a directory output, the input's filename is sanitized and
/// joined; with a file output the path is used directly; with no output the
/// input is overwritten in place. Fails when a directory output would produce
/// an unsafe filename. Missing directories are created only when the output is
/// actually written, so dry runs and failed inputs leave nothing behind.
fn resolve_target_output(file_path: &Path, output: &Option<PathBuf>) -> Result<PathBuf> {
    match output {
        Some(out) if is_output_dir(out) => {
            // Sanitize filename to prevent path traversal attacks
            sanitize_filename(file_path.file_name().unwrap_or(file_path.as_os_str()))
                .map(|safe_name| out.join(safe_name))
                .ok_or_else(|| anyhow!("Invalid filename {:?}", file_path.file_name()))
        }
        Some(out) => Ok(out.clone()),
        None => Ok(file_path.to_path_buf()),
    }
}

/// Print a per-file error on stderr. Progress bars are hidden when stderr is
/// not a terminal, so their messages cannot be the only place errors appear.
fn report_file_error(m: &MultiProgress, pb: &ProgressBar, file_path: &Path, e: &anyhow::Error) {
    pb.finish_and_clear();
    m.suspend(|| {
        eprintln!(
            "[{}] Error: {:#}",
            file_path
                .file_name()
                .unwrap_or(file_path.as_os_str())
                .to_string_lossy(),
            e
        )
    });
}

/// Turn the number of failed files of a batch into the command's result, so
/// the process exits non-zero when anything failed.
fn batch_result(failed: usize, total: usize) -> Result<()> {
    if failed > 0 {
        Err(anyhow!("{} of {} file(s) failed", failed, total))
    } else {
        Ok(())
    }
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

        /// Write tiled output. SIZE is N or WIDTHxHEIGHT, multiples of 16 (default: 512)
        #[arg(long, value_name = "SIZE", num_args = 0..=1, default_missing_value = "512", value_parser = parse_tile_size)]
        tile: Option<(u32, u32)>,

        /// Add internal overviews to the first page, e.g. 2,4,8,16 (block average; nearest for palettes)
        #[arg(long, value_name = "FACTORS", value_delimiter = ',', value_parser = clap::value_parser!(u32).range(2..))]
        overviews: Vec<u32>,

        /// Re-read the written file and check its pixels against the source before replacing anything
        #[arg(long)]
        checksum: bool,
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

/// Parse `--tile`: `N` (square) or `WIDTHxHEIGHT`, both multiples of 16 as the
/// TIFF specification requires for tile dimensions.
fn parse_tile_size(s: &str) -> std::result::Result<(u32, u32), String> {
    let (w, h) = s.split_once(['x', 'X']).unwrap_or((s, s));
    let parse = |v: &str| {
        v.trim()
            .parse::<u32>()
            .map_err(|_| format!("invalid tile size '{s}', expected N or WIDTHxHEIGHT"))
    };
    let (w, h) = (parse(w)?, parse(h)?);
    if w == 0 || h == 0 || !w.is_multiple_of(16) || !h.is_multiple_of(16) {
        return Err(format!(
            "tile dimensions must be non-zero multiples of 16, got {w}x{h}"
        ));
    }
    Ok((w, h))
}

/// Output layout options shared by every compression pass of a file.
#[derive(Clone, Debug, Default)]
struct OutputOptions {
    /// Tile size for tiled output; `None` writes one strip per image.
    tile: Option<(u32, u32)>,
    /// Overview decimation factors, ascending and deduplicated (each >= 2).
    overviews: Vec<u32>,
}

impl OutputOptions {
    fn new(tile: Option<(u32, u32)>, mut overviews: Vec<u32>) -> Self {
        overviews.sort_unstable();
        overviews.dedup();
        OutputOptions { tile, overviews }
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
    crate::metadata::install_tag_extender();

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
            tile,
            overviews,
            checksum,
        } => {
            let layout = OutputOptions::new(tile, overviews);
            compress_command(
                input, output, format, level, lossy, quantize, extreme, dry_run, benchmark, jobs,
                verbose, layout, checksum,
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
    layout: OutputOptions,
    checksum: bool,
) -> Result<()> {
    let files = expand_tiff_inputs(&input)?;

    check_batch_output(files.len(), &output)?;

    let m = MultiProgress::new();

    // Use rayon for file-level parallelism with configurable job count
    let num_jobs = jobs.unwrap_or_else(num_cpus::get);
    let failed = AtomicUsize::new(0);

    files
        .par_iter()
        .with_max_len(num_jobs)
        .for_each(|file_path| {
            let pb = new_file_progress(
                &m,
                format!(
                    "Processing {:?}",
                    file_path.file_name().unwrap_or(file_path.as_os_str())
                ),
            );

            let result = resolve_target_output(file_path, &output).and_then(|target_output| {
                process_single_file(
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
                    &layout,
                    checksum,
                    &pb,
                )
            });
            match result {
                Ok((original, compressed, best_fmt, is_dry_run)) => {
                    pb.finish();
                    // --extreme/--lossy print their own table; still report --checksum
                    if (!extreme && !lossy) || checksum {
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
                    failed.fetch_add(1, Ordering::Relaxed);
                    report_file_error(&m, &pb, file_path, &e);
                }
            }
        });

    batch_result(failed.into_inner(), files.len())
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
    layout: &OutputOptions,
    checksum: bool,
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
    let total_pages = count_tiff_pages(input).unwrap_or(0);

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

            // Only add to results if compression actually succeeded.
            // Trials keep the output layout (it affects size) but skip --checksum.
            if let Ok((size, _)) = run_compression_to_fd(
                input,
                temp_file,
                cid,
                pid,
                effective_level,
                quantize,
                verbose,
                total_pages,
                layout,
                false,
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

        let (dry_run_size, verification) = run_compression_to_fd(
            input,
            temp_file,
            cid,
            pid,
            effective_level,
            quantize,
            verbose,
            total_pages,
            layout,
            checksum,
            pb,
        )?;

        return Ok((
            original_size,
            dry_run_size,
            result_label(best_format, best_predictor, verification),
            true,
        ));
    }

    // Final compression with best format and predictor
    let cid = best_format.to_ffi();
    let pid = best_predictor.to_ffi();
    let verification = run_compression_pass(
        input,
        output,
        cid,
        pid,
        effective_level,
        quantize,
        verbose,
        total_pages,
        layout,
        checksum,
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
        result_label(best_format, best_predictor, verification),
        false,
    ))
}

/// Codec description for the per-file summary line, plus the `--checksum` outcome.
fn result_label(
    format: CompressionFormat,
    predictor: Predictor,
    verification: Option<Verification>,
) -> String {
    match verification {
        Some(v) => format!("{format}+{predictor}, {v}"),
        None => format!("{format}+{predictor}"),
    }
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

/// Count the number of IFDs (pages) in a TIFF. Returns at least 1 for a valid
/// file. Uses libtiff's `TIFFNumberOfDirectories`, which walks the directory
/// chain once internally and restores the current directory.
fn count_tiff_pages(path: &Path) -> Result<u16> {
    let c_path = CString::new(path.to_str().ok_or_else(|| anyhow!("Invalid path"))?)?;
    unsafe {
        let tif = TIFFOpen(c_path.as_ptr(), CString::new("r")?.as_ptr());
        if tif.is_null() {
            return Err(anyhow!("Failed to open TIFF file: {:?}", path));
        }
        let pages = TIFFNumberOfDirectories(tif);
        TIFFClose(tif);
        Ok(pages)
    }
}

/// An open libtiff handle, closed on drop so error paths cannot leak it.
struct TiffHandle(*mut TIFF);

impl TiffHandle {
    fn open(path: &Path, mode: &str) -> Result<Self> {
        let c_path = CString::new(path.to_str().ok_or_else(|| anyhow!("Invalid path"))?)?;
        let c_mode = CString::new(mode)?;
        let tif = unsafe { TIFFOpen(c_path.as_ptr(), c_mode.as_ptr()) };
        if tif.is_null() {
            return Err(anyhow!("Failed to open TIFF {:?} (mode {})", path, mode));
        }
        Ok(TiffHandle(tif))
    }

    /// Open a libtiff handle on a duplicate of `file`'s descriptor; libtiff
    /// closes the duplicate, `file` stays usable.
    fn from_file(file: &std::fs::File, mode: &str) -> Result<Self> {
        let c_mode = CString::new(mode)?;
        unsafe {
            let fd = libc::dup(file.as_raw_fd());
            if fd < 0 {
                return Err(anyhow!("Failed to duplicate file descriptor"));
            }
            // libtiff reads the header at the current offset, which the
            // duplicate shares with `file`
            libc::lseek(fd, 0, libc::SEEK_SET);
            let tif = TIFFFdOpen(fd, c"dry_run".as_ptr(), c_mode.as_ptr());
            if tif.is_null() {
                libc::close(fd);
                return Err(anyhow!(
                    "Failed to open TIFF on file descriptor (mode {})",
                    mode
                ));
            }
            Ok(TiffHandle(tif))
        }
    }

    fn ptr(&self) -> *mut TIFF {
        self.0
    }
}

impl Drop for TiffHandle {
    fn drop(&mut self) {
        unsafe { TIFFClose(self.0) };
    }
}

/// Temporary output file, deleted on drop unless `persist` renamed it.
struct TempOutput {
    path: PathBuf,
    persisted: bool,
}

impl TempOutput {
    fn new(path: PathBuf) -> Self {
        TempOutput {
            path,
            persisted: false,
        }
    }

    fn persist(mut self, target: &Path) -> Result<()> {
        fs::rename(&self.path, target)
            .with_context(|| format!("Failed to rename {:?} to {:?}", self.path, target))?;
        self.persisted = true;
        Ok(())
    }
}

impl Drop for TempOutput {
    fn drop(&mut self) {
        if !self.persisted {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// libtiff write mode: BigTIFF when the input is large enough that the output
/// could exceed the classic 4 GiB limit.
fn write_mode(input: &Path) -> Result<&'static str> {
    Ok(if input.metadata()?.len() > 4 * 1024 * 1024 * 1024 {
        "w8"
    } else {
        "w"
    })
}

/// Encode every page of `tif_src` into `tif_dst`. Returns what was written for
/// each output IFD (base images and generated overviews), for `--checksum`.
#[allow(clippy::too_many_arguments)]
unsafe fn compress_pages(
    input: &Path,
    tif_src: *mut TIFF,
    tif_dst: *mut TIFF,
    compression: u16,
    predictor: u16,
    level: Option<u32>,
    quantize: bool,
    verbose: bool,
    total_pages: u16,
    layout: &OutputOptions,
    pb: &ProgressBar,
) -> Result<Vec<WrittenIfd>> {
    let mut written = Vec::new();
    let mut page = 0;
    loop {
        if verbose {
            log::info!("Processing IFD {}", page);
        }
        pb.set_message(format!("Page {}/{}", page + 1, total_pages));
        pb.set_position(((page as u64) * 100) / (total_pages.max(1) as u64));

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
            layout,
            &mut written,
            pb,
        )?;

        if TIFFReadDirectory(tif_src) == 0 {
            break;
        }
        page += 1;
    }
    Ok(written)
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
    layout: &OutputOptions,
    checksum: bool,
    pb: &ProgressBar,
) -> Result<Option<Verification>> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create output directory {:?}", parent))?;
    }
    let tmp = TempOutput::new(output.with_extension("tmp_tiffreducer"));
    let written = {
        let src = TiffHandle::open(input, "r")?;
        let dst = TiffHandle::open(&tmp.path, write_mode(input)?)?;
        unsafe {
            compress_pages(
                input,
                src.ptr(),
                dst.ptr(),
                compression,
                predictor,
                level,
                quantize,
                verbose,
                total_pages,
                layout,
                pb,
            )?
        }
        // Both handles close here, flushing the output before it is re-read.
    };

    let verification = if checksum {
        pb.set_message("Verifying checksum");
        let out = TiffHandle::open(&tmp.path, "r")?;
        Some(unsafe { verify_output(input, out.ptr(), &written, compression)? })
    } else {
        None
    };

    // Only now may the target (possibly the input itself) be replaced.
    tmp.persist(output)?;
    Ok(verification)
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
    layout: &OutputOptions,
    checksum: bool,
    pb: &ProgressBar,
) -> Result<(u64, Option<Verification>)> {
    let written = {
        let src = TiffHandle::open(input, "r")?;
        let dst = TiffHandle::from_file(&output_file, write_mode(input)?)?;
        unsafe {
            compress_pages(
                input,
                src.ptr(),
                dst.ptr(),
                compression,
                predictor,
                level,
                quantize,
                verbose,
                total_pages,
                layout,
                pb,
            )?
        }
    };

    let verification = if checksum {
        let out = TiffHandle::from_file(&output_file, "r")?;
        Some(unsafe { verify_output(input, out.ptr(), &written, compression)? })
    } else {
        None
    };

    use std::io::{Seek, SeekFrom};
    let mut f = output_file;
    let size = f.seek(SeekFrom::End(0))?;
    Ok((size, verification))
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
    layout: &OutputOptions,
    written: &mut Vec<WrittenIfd>,
    pb: &ProgressBar,
) -> Result<()> {
    // Existing reduced-resolution IFDs are dropped when overviews are regenerated
    let mut subfile_type: u32 = 0;
    TIFFGetField(tif_src, TIFFTAG_SUBFILETYPE, &mut subfile_type);
    if !layout.overviews.is_empty() && page_index > 0 && subfile_type & FILETYPE_REDUCEDIMAGE != 0 {
        if verbose {
            log::info!(
                "Skipping existing overview IFD {} (overviews are regenerated)",
                page_index
            );
        }
        return Ok(());
    }

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

    set_output_layout(tif_dst, layout.tile, h);

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

    apply_codec_level(tif_dst, compression, level);

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

    let target = ImageFormat {
        bps: target_bps,
        spp,
        fmt: target_fmt,
        photometric,
        planar,
        compression,
        predictor: final_predictor,
        level,
    };

    // Overviews of the first page are accumulated while its rows stream out
    let overviews = if !layout.overviews.is_empty() && page_index == 0 {
        let nodata = if quantize {
            None // the source nodata value means nothing after quantization
        } else {
            crate::ffi::get_gdal_nodata(tif_src)
        };
        let builder = OverviewBuilder::new(w, h, &target, &layout.overviews, nodata);
        if builder.is_none() {
            log::warn!(
                "{}: overviews are not supported for {}-bit samples (format {}); none written",
                input_path.display(),
                target_bps,
                target_fmt
            );
        }
        builder
    } else {
        None
    };

    let mut out = RowWriter::new(
        tif_dst,
        w,
        h,
        TIFFScanlineSize(tif_dst) as usize,
        target.bits_per_pixel(),
        layout.tile,
        overviews,
    );

    if is_tiled {
        if verbose {
            pb.println("Image is tiled, using parallel tiled processing path");
        }
        process_tiled_image(
            input_path,
            tif_src,
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
            &mut out,
            pb,
        )?;
    } else {
        if verbose {
            pb.println("Image is striped, using striped processing path");
        }
        process_striped_image(
            tif_src, w, h, spp, bps, fmt, planar, quantize, verbose, &mut out, pb,
        )?;
    }

    if TIFFWriteDirectory(tif_dst) == 0 {
        return Err(anyhow!("Failed to write directory for page {}", page_index));
    }
    let (digest, overviews) = out.finish();
    written.push(WrittenIfd {
        digest,
        // Without quantization the output must match the decoded source page
        source_page: (!quantize).then_some(page_index),
    });

    if let Some(builder) = overviews {
        write_overviews(
            tif_src,
            tif_dst,
            builder,
            &target,
            layout.tile,
            written,
            verbose,
            pb,
        )?;
    }

    Ok(())
}

/// Create a scanline filled with nodata value for the given format
fn create_nodata_scanline(
    nodata: f64,
    bps: u16,
    fmt: u16,
    w: u32,
    spp: u16,
    planar: u16,
    buf_template: &[u8],
) -> Vec<u8> {
    let mut nodata_buf = vec![0u8; buf_template.len()];
    let spp_eff = if planar == PLANARCONFIG_SEPARATE {
        1
    } else {
        spp as u32
    };
    let pixel_count = (w * spp_eff) as usize;

    if bps == 32 && fmt == SAMPLEFORMAT_IEEEFP {
        let slice = unsafe {
            std::slice::from_raw_parts_mut(nodata_buf.as_mut_ptr() as *mut f32, pixel_count)
        };
        slice.fill(nodata as f32);
    } else if bps == 64 && fmt == SAMPLEFORMAT_IEEEFP {
        let slice = unsafe {
            std::slice::from_raw_parts_mut(nodata_buf.as_mut_ptr() as *mut f64, pixel_count)
        };
        slice.fill(nodata);
    } else if bps == 16 && fmt == SAMPLEFORMAT_INT {
        let slice = unsafe {
            std::slice::from_raw_parts_mut(nodata_buf.as_mut_ptr() as *mut i16, pixel_count)
        };
        slice.fill(nodata as i16);
    } else if bps == 16 && fmt == SAMPLEFORMAT_UINT {
        let slice = unsafe {
            std::slice::from_raw_parts_mut(nodata_buf.as_mut_ptr() as *mut u16, pixel_count)
        };
        slice.fill(nodata as u16);
    } else if bps == 8 && fmt == SAMPLEFORMAT_UINT {
        let slice = unsafe { std::slice::from_raw_parts_mut(nodata_buf.as_mut_ptr(), pixel_count) };
        slice.fill(nodata as u8);
    } else {
        // Default: fill with zeros
        nodata_buf.fill(0);
    }
    nodata_buf
}

#[allow(clippy::too_many_arguments)]
unsafe fn process_striped_image(
    tif_src: *mut TIFF,
    w: u32,
    h: u32,
    spp: u16,
    bps: u16,
    fmt: u16,
    planar: u16,
    quantize: bool,
    verbose: bool,
    out: &mut RowWriter,
    pb: &ProgressBar,
) -> Result<()> {
    const MAX_SCANLINE_SIZE: usize = 1024 * 1024 * 1024;
    let in_scanline = TIFFScanlineSize(tif_src) as usize;

    if in_scanline == 0 || in_scanline > MAX_SCANLINE_SIZE {
        return Err(anyhow!("Invalid scanline size: {}", in_scanline));
    }

    // Get GDAL nodata value for sparse strip filling
    let nodata = crate::ffi::get_gdal_nodata(tif_src).unwrap_or(0.0);

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

    // Pre-compute nodata bytes for the scanline
    let nodata_bytes = create_nodata_scanline(nodata, bps, fmt, w, spp, planar, &buf_in);

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

            // Sparse strips (no data written by GDAL) read as nodata
            if crate::ffi::is_strile_sparse(tif_src, TIFFComputeStrip(tif_src, row, s)) {
                // Fill buffer with nodata value
                buf_in.copy_from_slice(&nodata_bytes);
            } else if TIFFReadScanline(tif_src, buf_in.as_mut_ptr() as *mut _, row, s) < 0 {
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
                out.write_row(row, s, &buf_out)?;
            } else {
                out.write_row(row, s, &buf_in)?;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
unsafe fn process_tiled_image(
    input_path: &Path, // Need path to open more handles
    tif_src: *mut TIFF,
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
    out: &mut RowWriter,
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

    // Channels packed into one pixel for PLANARCONFIG_CONTIG; one per plane for SEPARATE.
    let pixel_spp = if planar == PLANARCONFIG_SEPARATE {
        1
    } else {
        spp as usize
    };
    // Work in packed bits so sub-byte (1/2/4-bit) tiled data unpacks correctly.
    // For bps >= 8 this is identical to the previous byte-per-pixel layout.
    let bits_per_pixel = (bps as usize) * pixel_spp;
    let in_row_size = (w as usize * bits_per_pixel).div_ceil(8);

    // Sub-byte data is only handled when each tile row is a whole number of bytes,
    // so tiles stay byte-aligned in the destination row. A non-aligned sub-byte
    // tile width would require bit-shifting to merge tiles, which we don't
    // implement; reject it rather than silently corrupt the image.
    if !bits_per_pixel.is_multiple_of(8)
        && !(tile_width as usize * bits_per_pixel).is_multiple_of(8)
    {
        return Err(anyhow!(
            "Sub-byte tiled images with non-byte-aligned tile rows are not supported \
             (bps={}, tile_width={})",
            bps,
            tile_width
        ));
    }

    if in_row_size > MAX_SCANLINE_SIZE {
        return Err(anyhow!("Input row size too large"));
    }

    let out_row_size = if quantize {
        (w as usize) * pixel_spp
    } else {
        in_row_size
    };

    let image_strip_size = in_row_size * (tile_length as usize);
    let mut image_strip = vec![0u8; image_strip_size];

    let geom = TileGeometry::new(w, h, tile_width, tile_length);
    let tiles_down = geom.tiles_down;

    let tile_row_size = (tile_width as usize * bits_per_pixel).div_ceil(8);
    let tile_buffer_size = tile_row_size * (tile_length as usize);

    // Worker threads decode with their own handles on this file and page
    let c_path = CString::new(
        input_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid input path"))?,
    )?;

    // Sparse tiles (no data written by GDAL) read as nodata, like sparse strips
    let nodata = crate::ffi::get_gdal_nodata(tif_src).unwrap_or(0.0);
    let nodata_row =
        create_nodata_scanline(nodata, bps, fmt, w, spp, planar, &vec![0u8; in_row_size]);

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
            let tile_indices: Vec<(u32, u32, bool)> = (0..geom.tiles_across)
                .map(|tile_x| {
                    let tile_index =
                        geom.tile_index(tile_x, tile_y, s as u32, planar == PLANARCONFIG_SEPARATE);
                    let sparse = crate::ffi::is_strile_sparse(tif_src, tile_index);
                    (tile_x, tile_index, sparse)
                })
                .collect();

            // Parallel decode tiles (`None` = sparse tile)
            let decoded_tiles: Vec<(u32, Option<Vec<u8>>)> = tile_indices
                .par_iter()
                .map(
                    |&(tile_x, tile_index, sparse)| -> Result<(u32, Option<Vec<u8>>)> {
                        if sparse {
                            return Ok((tile_x, None));
                        }
                        let mut buf = vec![0u8; tile_buffer_size];
                        let read = with_worker_handle(&c_path, page_index, |tif| unsafe {
                            crate::ffi::TIFFReadEncodedTile(
                                tif,
                                tile_index,
                                buf.as_mut_ptr() as *mut _,
                                tile_buffer_size as isize,
                            )
                        })?;
                        if read < 0 {
                            return Err(anyhow!(
                                "Failed to decode tile {} of page {}",
                                tile_index,
                                page_index
                            ));
                        }
                        Ok((tile_x, Some(buf)))
                    },
                )
                .collect::<Result<_>>()?;

            // Assembly (Sequential)
            for (tile_x, tile_buf) in decoded_tiles {
                let start_x = (tile_x as usize) * (tile_width as usize);
                let actual_width = std::cmp::min(tile_width as usize, w as usize - start_x);
                let actual_height = std::cmp::min(
                    tile_length as usize,
                    h as usize - (tile_y as usize * tile_length as usize),
                );

                // Byte-aligned because the guard above ensures tile rows (and
                // hence every tile's start column) fall on byte boundaries.
                let col_start = (start_x * bits_per_pixel) / 8;
                let copy_len = (actual_width * bits_per_pixel).div_ceil(8);
                for row in 0..actual_height {
                    let src = match &tile_buf {
                        Some(buf) => &buf[row * tile_row_size..][..copy_len],
                        None => &nodata_row[col_start..][..copy_len],
                    };
                    let dst_start = row * in_row_size + col_start;
                    image_strip[dst_start..dst_start + copy_len].copy_from_slice(src);
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
                    out.write_row(global_row, s, out_buf)?;
                } else {
                    let row_start = row_idx * in_row_size;
                    out.write_row(
                        global_row,
                        s,
                        &image_strip[row_start..row_start + in_row_size],
                    )?;
                }
            }
        }
    }
    Ok(())
}

/// Pixel and codec settings of an output IFD.
#[derive(Clone, Copy)]
struct ImageFormat {
    bps: u16,
    spp: u16,
    fmt: u16,
    photometric: u16,
    planar: u16,
    compression: u16,
    predictor: u16,
    level: Option<u32>,
}

impl ImageFormat {
    /// Samples per pixel within one row: all of them when contiguous, one per plane otherwise.
    fn row_spp(&self) -> usize {
        if self.planar == PLANARCONFIG_SEPARATE {
            1
        } else {
            self.spp as usize
        }
    }

    fn planes(&self) -> u16 {
        if self.planar == PLANARCONFIG_SEPARATE {
            self.spp
        } else {
            1
        }
    }

    fn bits_per_pixel(&self) -> usize {
        self.bps as usize * self.row_spp()
    }
}

/// Set the codec quality/level tag for `compression` (the predictor is set separately).
unsafe fn apply_codec_level(tif: *mut TIFF, compression: u16, level: Option<u32>) {
    if let Some(lvl) = level {
        match compression {
            COMPRESSION_LZMA => {
                TIFFSetField(tif, TIFFTAG_LZMAPRESET, lvl.clamp(1, 9) as i32);
            }
            COMPRESSION_ZSTD => {
                let clamped: i32 = lvl.clamp(1, 22) as i32;
                TIFFSetField(tif, TIFFTAG_ZSTD_LEVEL, clamped);
            }
            COMPRESSION_JPEGXL | COMPRESSION_JPEG | COMPRESSION_WEBP => {
                let tag = match compression {
                    COMPRESSION_JPEGXL => TIFFTAG_DEFLATELEVEL,
                    COMPRESSION_JPEG => TIFFTAG_JPEGQUALITY,
                    COMPRESSION_WEBP => TIFFTAG_WEBP_LEVEL,
                    _ => unreachable!(),
                };
                TIFFSetField(tif, tag, lvl.clamp(1, 100) as i32);
            }
            _ => {}
        }
    }
}

/// Configure tiles of the given size, or a single strip of `h` rows (striped
/// output even if the source is tiled).
unsafe fn set_output_layout(tif: *mut TIFF, tile: Option<(u32, u32)>, h: u32) {
    match tile {
        Some((tw, th)) => {
            TIFFSetField(tif, TIFFTAG_TILEWIDTH, tw);
            TIFFSetField(tif, TIFFTAG_TILELENGTH, th);
        }
        None => {
            TIFFSetField(tif, TIFFTAG_ROWSPERSTRIP, h);
        }
    }
}

/// Destination for the decoded rows of one output IFD, fed in order (plane by
/// plane for separate planar data). Writes scanlines, or collects a band of
/// rows and cuts it into tiles. Every row is also fingerprinted for
/// `--checksum` and handed to the overview builder, if any.
struct RowWriter {
    tif: *mut TIFF,
    w: u32,
    h: u32,
    row_bytes: usize,
    bits_per_pixel: usize,
    tiles: Option<TileBand>,
    hasher: DefaultHasher,
    overviews: Option<OverviewBuilder>,
}

/// Rows buffered until a full row of tiles can be written.
struct TileBand {
    width: u32,
    height: u32,
    /// Bytes per tile row
    row_bytes: usize,
    /// `height` image rows
    band: Vec<u8>,
    tile: Vec<u8>,
}

impl RowWriter {
    fn new(
        tif: *mut TIFF,
        w: u32,
        h: u32,
        row_bytes: usize,
        bits_per_pixel: usize,
        tile: Option<(u32, u32)>,
        overviews: Option<OverviewBuilder>,
    ) -> Self {
        let tiles = tile.map(|(width, height)| {
            let tile_row_bytes = (width as usize * bits_per_pixel).div_ceil(8);
            TileBand {
                width,
                height,
                row_bytes: tile_row_bytes,
                band: vec![0; row_bytes * height as usize],
                tile: vec![0; tile_row_bytes * height as usize],
            }
        });
        RowWriter {
            tif,
            w,
            h,
            row_bytes,
            bits_per_pixel,
            tiles,
            hasher: DefaultHasher::new(),
            overviews,
        }
    }

    unsafe fn write_row(&mut self, row: u32, sample: u16, data: &[u8]) -> Result<()> {
        let data = data.get(..self.row_bytes).ok_or_else(|| {
            anyhow!(
                "Row {} is shorter than a scanline ({} < {} bytes)",
                row,
                data.len(),
                self.row_bytes
            )
        })?;
        self.hasher.write(data);
        if let Some(overviews) = &mut self.overviews {
            overviews.push_row(sample, row, data);
        }

        let Some(t) = &mut self.tiles else {
            if TIFFWriteScanline(self.tif, data.as_ptr() as *mut _, row, sample) < 0 {
                return Err(anyhow!(
                    "Failed to write scanline {} (sample {})",
                    row,
                    sample
                ));
            }
            return Ok(());
        };

        let band_row = (row % t.height) as usize;
        t.band[band_row * self.row_bytes..][..self.row_bytes].copy_from_slice(data);
        if band_row + 1 < t.height as usize && row + 1 < self.h {
            return Ok(());
        }

        // Band complete: cut it into tiles, zero-padded past the image edges
        let y = row - band_row as u32;
        for x in (0..self.w).step_by(t.width as usize) {
            // Tile widths are multiples of 16, so every tile starts on a byte
            let start = x as usize * self.bits_per_pixel / 8;
            let len = t.row_bytes.min(self.row_bytes - start);
            t.tile.fill(0);
            for r in 0..=band_row {
                t.tile[r * t.row_bytes..][..len]
                    .copy_from_slice(&t.band[r * self.row_bytes + start..][..len]);
            }
            if TIFFWriteTile(self.tif, t.tile.as_mut_ptr() as *mut _, x, y, 0, sample) < 0 {
                return Err(anyhow!(
                    "Failed to write tile at ({}, {}) (sample {})",
                    x,
                    y,
                    sample
                ));
            }
        }
        Ok(())
    }

    /// Fingerprint of every row written, and the overview builder if any.
    fn finish(self) -> (u64, Option<OverviewBuilder>) {
        (self.hasher.finish(), self.overviews)
    }
}

/// Sample types overviews can be computed for.
#[derive(Clone, Copy)]
enum SampleKind {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    F32,
    F64,
}

impl SampleKind {
    fn new(bps: u16, fmt: u16) -> Option<Self> {
        let uint = fmt == SAMPLEFORMAT_UINT || fmt == 0;
        let int = fmt == SAMPLEFORMAT_INT;
        let float = fmt == SAMPLEFORMAT_IEEEFP;
        Some(match bps {
            8 if uint => Self::U8,
            8 if int => Self::I8,
            16 if uint => Self::U16,
            16 if int => Self::I16,
            32 if uint => Self::U32,
            32 if int => Self::I32,
            32 if float => Self::F32,
            64 if float => Self::F64,
            _ => return None,
        })
    }

    fn size(self) -> usize {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::F64 => 8,
        }
    }

    /// Sample `i` of a native-endian row.
    fn get(self, b: &[u8], i: usize) -> f64 {
        fn bytes<const N: usize>(b: &[u8], i: usize) -> [u8; N] {
            b[i * N..(i + 1) * N].try_into().unwrap()
        }
        match self {
            Self::U8 => b[i] as f64,
            Self::I8 => b[i] as i8 as f64,
            Self::U16 => u16::from_ne_bytes(bytes(b, i)) as f64,
            Self::I16 => i16::from_ne_bytes(bytes(b, i)) as f64,
            Self::U32 => u32::from_ne_bytes(bytes(b, i)) as f64,
            Self::I32 => i32::from_ne_bytes(bytes(b, i)) as f64,
            Self::F32 => f32::from_ne_bytes(bytes(b, i)) as f64,
            Self::F64 => f64::from_ne_bytes(bytes(b, i)),
        }
    }

    /// Store `v` as sample `i`, rounded and saturated for integer types.
    fn put(self, b: &mut [u8], i: usize, v: f64) {
        let size = self.size();
        let dst = &mut b[i * size..(i + 1) * size];
        match self {
            Self::U8 => dst[0] = v.round() as u8,
            Self::I8 => dst[0] = v.round() as i8 as u8,
            Self::U16 => dst.copy_from_slice(&(v.round() as u16).to_ne_bytes()),
            Self::I16 => dst.copy_from_slice(&(v.round() as i16).to_ne_bytes()),
            Self::U32 => dst.copy_from_slice(&(v.round() as u32).to_ne_bytes()),
            Self::I32 => dst.copy_from_slice(&(v.round() as i32).to_ne_bytes()),
            Self::F32 => dst.copy_from_slice(&(v as f32).to_ne_bytes()),
            Self::F64 => dst.copy_from_slice(&v.to_ne_bytes()),
        }
    }
}

/// One reduced-resolution image being accumulated.
struct OverviewLevel {
    factor: u32,
    w: u32,
    h: u32,
    /// Finished rows: one block of `h` rows per plane
    data: Vec<u8>,
    /// Per-sample sums and counts of the output row in progress
    sums: Vec<f64>,
    counts: Vec<u32>,
}

/// Builds overviews from full-resolution rows as they stream past, GDAL
/// AVERAGE-style: each output sample is the mean of its factor x factor block,
/// ignoring NaN and nodata samples. Palette images use the block's top-left
/// sample instead, since averaging indices is meaningless.
struct OverviewBuilder {
    w: u32,
    h: u32,
    row_spp: usize,
    kind: SampleKind,
    nearest: bool,
    nodata: Option<f64>,
    levels: Vec<OverviewLevel>,
}

impl OverviewBuilder {
    /// `None` if the sample type is not supported (sub-byte, 24-bit, 64-bit integers).
    fn new(
        w: u32,
        h: u32,
        format: &ImageFormat,
        factors: &[u32],
        nodata: Option<f64>,
    ) -> Option<Self> {
        let kind = SampleKind::new(format.bps, format.fmt)?;
        let row_spp = format.row_spp();
        let levels = factors
            .iter()
            .map(|&factor| {
                let (ow, oh) = (w.div_ceil(factor), h.div_ceil(factor));
                let n = ow as usize * row_spp;
                OverviewLevel {
                    factor,
                    w: ow,
                    h: oh,
                    data: vec![0; n * kind.size() * oh as usize * format.planes() as usize],
                    sums: vec![0.0; n],
                    counts: vec![0; n],
                }
            })
            .collect();
        Some(OverviewBuilder {
            w,
            h,
            row_spp,
            kind,
            nearest: format.photometric == PHOTOMETRIC_PALETTE,
            nodata,
            levels,
        })
    }

    fn push_row(&mut self, plane: u16, row: u32, data: &[u8]) {
        let (kind, n) = (self.kind, self.row_spp);
        let px = n * kind.size();
        for lvl in &mut self.levels {
            let f = lvl.factor as usize;
            let row_bytes = lvl.w as usize * px;
            let out_row = plane as usize * lvl.h as usize + (row / lvl.factor) as usize;
            let dst = &mut lvl.data[out_row * row_bytes..][..row_bytes];

            if self.nearest {
                if row.is_multiple_of(lvl.factor) {
                    for ox in 0..lvl.w as usize {
                        dst[ox * px..][..px].copy_from_slice(&data[ox * f * px..][..px]);
                    }
                }
                continue;
            }

            for x in 0..self.w as usize {
                let ox = x / f;
                for c in 0..n {
                    let v = kind.get(data, x * n + c);
                    if v.is_nan() || self.nodata == Some(v) {
                        continue;
                    }
                    lvl.sums[ox * n + c] += v;
                    lvl.counts[ox * n + c] += 1;
                }
            }

            if (row + 1).is_multiple_of(lvl.factor) || row + 1 == self.h {
                // Blocks without a valid sample become nodata (NaN if there is none)
                let empty = self.nodata.unwrap_or(f64::NAN);
                for (k, (sum, count)) in lvl.sums.iter_mut().zip(&mut lvl.counts).enumerate() {
                    let v = if *count > 0 {
                        *sum / *count as f64
                    } else {
                        empty
                    };
                    kind.put(dst, k, v);
                    *sum = 0.0;
                    *count = 0;
                }
            }
        }
    }
}

/// Write the accumulated overviews as reduced-resolution IFDs following the
/// base image, with its pixel format, codec and layout.
#[allow(clippy::too_many_arguments)]
unsafe fn write_overviews(
    tif_src: *mut TIFF,
    tif_dst: *mut TIFF,
    builder: OverviewBuilder,
    format: &ImageFormat,
    tile: Option<(u32, u32)>,
    written: &mut Vec<WrittenIfd>,
    verbose: bool,
    pb: &ProgressBar,
) -> Result<()> {
    for lvl in &builder.levels {
        if verbose {
            pb.println(format!(
                "Writing overview 1/{}: {}x{}",
                lvl.factor, lvl.w, lvl.h
            ));
        }
        TIFFSetField(tif_dst, TIFFTAG_SUBFILETYPE, FILETYPE_REDUCEDIMAGE);
        TIFFSetField(tif_dst, TIFFTAG_IMAGEWIDTH, lvl.w);
        TIFFSetField(tif_dst, TIFFTAG_IMAGELENGTH, lvl.h);
        TIFFSetField(tif_dst, TIFFTAG_BITSPERSAMPLE, format.bps as u32);
        TIFFSetField(tif_dst, TIFFTAG_SAMPLESPERPIXEL, format.spp as u32);
        if format.fmt != 0 {
            TIFFSetField(tif_dst, TIFFTAG_SAMPLEFORMAT, format.fmt as u32);
        }
        TIFFSetField(tif_dst, TIFFTAG_PHOTOMETRIC, format.photometric as u32);
        if format.spp > 1 {
            TIFFSetField(tif_dst, TIFFTAG_PLANARCONFIG, format.planar as u32);
        }
        crate::metadata::copy_extrasamples(tif_src, tif_dst)?;
        match format.photometric {
            PHOTOMETRIC_PALETTE => crate::metadata::copy_colormap(tif_src, tif_dst)?,
            PHOTOMETRIC_YCBCR => crate::metadata::copy_ycbcr_tags(tif_src, tif_dst)?,
            _ => {}
        }
        set_output_layout(tif_dst, tile, lvl.h);
        TIFFSetField(tif_dst, TIFFTAG_COMPRESSION, format.compression as i32);
        apply_codec_level(tif_dst, format.compression, format.level);
        if format.predictor != PREDICTOR_NONE {
            TIFFSetField(tif_dst, TIFFTAG_PREDICTOR, format.predictor as u32);
        }

        let row_bytes = lvl.w as usize * builder.row_spp * builder.kind.size();
        let mut out = RowWriter::new(
            tif_dst,
            lvl.w,
            lvl.h,
            row_bytes,
            format.bits_per_pixel(),
            tile,
            None,
        );
        for (plane, rows) in lvl
            .data
            .chunks_exact(row_bytes * lvl.h as usize)
            .enumerate()
        {
            for (row, data) in rows.chunks_exact(row_bytes).enumerate() {
                out.write_row(row as u32, plane as u16, data)?;
            }
        }
        if TIFFWriteDirectory(tif_dst) == 0 {
            return Err(anyhow!(
                "Failed to write overview directory (factor {})",
                lvl.factor
            ));
        }
        written.push(WrittenIfd {
            digest: out.finish().0,
            source_page: None,
        });
    }
    Ok(())
}

/// What was encoded into one output IFD, for `--checksum`.
struct WrittenIfd {
    /// Fingerprint of the rows handed to libtiff
    digest: u64,
    /// Source page this IFD must decode identically to (none for overviews
    /// and quantized pages)
    source_page: Option<u16>,
}

/// Outcome of `--checksum`.
#[derive(Clone, Copy, Debug)]
enum Verification {
    /// Every IFD decodes to exactly the expected pixels.
    Exact,
    /// Lossy codec: every IFD decodes, pixel values are not compared.
    DecodedOnly,
}

impl std::fmt::Display for Verification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verification::Exact => write!(f, "checksum OK"),
            Verification::DecodedOnly => {
                write!(f, "checksum: decodes OK, pixels not compared (lossy)")
            }
        }
    }
}

fn is_lossless(compression: u16) -> bool {
    matches!(
        compression,
        COMPRESSION_NONE
            | COMPRESSION_LZW
            | COMPRESSION_ADOBE_DEFLATE
            | COMPRESSION_DEFLATE
            | COMPRESSION_ZSTD
            | COMPRESSION_LZMA
            | COMPRESSION_PACKBITS
    )
}

/// Re-read every IFD of the finished output and, for lossless codecs, compare
/// it with what was encoded and with an independent decode of its source page.
/// The latter catches bugs in the read side of the pipeline, not just in
/// libtiff's encode/decode round trip.
unsafe fn verify_output(
    input: &Path,
    out: *mut TIFF,
    written: &[WrittenIfd],
    compression: u16,
) -> Result<Verification> {
    let exact = is_lossless(compression);
    let src = if exact && written.iter().any(|w| w.source_page.is_some()) {
        Some(TiffHandle::open(input, "r")?)
    } else {
        None
    };

    let mut index = 0;
    loop {
        let expected = written.get(index).ok_or_else(|| {
            anyhow!(
                "Checksum: output has more IFDs than the {} written",
                written.len()
            )
        })?;
        let digest = ifd_digest(out)
            .map_err(|e| anyhow!("Checksum: output IFD {} cannot be decoded: {:#}", index, e))?;
        if exact {
            if digest != expected.digest {
                return Err(anyhow!(
                    "Checksum mismatch: output IFD {} does not decode to the pixels that were encoded",
                    index
                ));
            }
            if let (Some(page), Some(src)) = (expected.source_page, &src) {
                if TIFFSetDirectory(src.ptr(), page) == 0 {
                    return Err(anyhow!("Checksum: cannot re-read source page {}", page));
                }
                let source = ifd_digest(src.ptr()).map_err(|e| {
                    anyhow!("Checksum: source page {} cannot be decoded: {:#}", page, e)
                })?;
                if source != digest {
                    return Err(anyhow!(
                        "Checksum mismatch: output IFD {} differs from source page {}",
                        index,
                        page
                    ));
                }
            }
        }
        index += 1;
        if TIFFReadDirectory(out) == 0 {
            break;
        }
    }

    if index != written.len() {
        return Err(anyhow!(
            "Checksum: output has {} IFDs but {} were written",
            index,
            written.len()
        ));
    }
    Ok(if exact {
        Verification::Exact
    } else {
        Verification::DecodedOnly
    })
}

/// Fingerprint the decoded pixels of the current directory, row by row in the
/// order `RowWriter` receives them. Deliberately independent of the
/// compression pipeline: plain scanline/tile reads, nodata for sparse strips
/// and tiles.
unsafe fn ifd_digest(tif: *mut TIFF) -> Result<u64> {
    let (mut w, mut h) = (0u32, 0u32);
    let (mut bps, mut spp, mut fmt, mut planar) = (0u16, 0u16, 0u16, 0u16);
    TIFFGetField(tif, TIFFTAG_IMAGEWIDTH, &mut w);
    TIFFGetField(tif, TIFFTAG_IMAGELENGTH, &mut h);
    TIFFGetField(tif, TIFFTAG_BITSPERSAMPLE, &mut bps);
    TIFFGetField(tif, TIFFTAG_SAMPLESPERPIXEL, &mut spp);
    TIFFGetField(tif, TIFFTAG_SAMPLEFORMAT, &mut fmt);
    TIFFGetField(tif, TIFFTAG_PLANARCONFIG, &mut planar);
    let spp = spp.max(1);
    let (planes, row_spp) = if planar == PLANARCONFIG_SEPARATE {
        (spp, 1)
    } else {
        (1, spp as usize)
    };
    let row_bytes = TIFFScanlineSize(tif) as usize;
    if row_bytes == 0 {
        return Err(anyhow!("invalid scanline size"));
    }
    let nodata = crate::ffi::get_gdal_nodata(tif).unwrap_or(0.0);
    let nodata_row =
        create_nodata_scanline(nodata, bps, fmt, w, spp, planar, &vec![0u8; row_bytes]);
    let mut hasher = DefaultHasher::new();

    if TIFFIsTiled(tif) == 0 {
        let mut row_buf = vec![0u8; row_bytes];
        for s in 0..planes {
            for row in 0..h {
                if crate::ffi::is_strile_sparse(tif, TIFFComputeStrip(tif, row, s)) {
                    hasher.write(&nodata_row);
                } else if TIFFReadScanline(tif, row_buf.as_mut_ptr() as *mut _, row, s) < 0 {
                    return Err(anyhow!("failed to read scanline {} (sample {})", row, s));
                } else {
                    hasher.write(&row_buf);
                }
            }
        }
        return Ok(hasher.finish());
    }

    let (mut tw, mut th) = (0u32, 0u32);
    TIFFGetField(tif, TIFFTAG_TILEWIDTH, &mut tw);
    TIFFGetField(tif, TIFFTAG_TILELENGTH, &mut th);
    let bits_per_pixel = bps as usize * row_spp;
    if tw == 0 || th == 0 || !(tw as usize * bits_per_pixel).is_multiple_of(8) {
        return Err(anyhow!(
            "unsupported tile layout {}x{} at {} bits per pixel",
            tw,
            th,
            bits_per_pixel
        ));
    }
    let tile_row_bytes = tw as usize * bits_per_pixel / 8;
    let mut tile_buf = vec![0u8; (TIFFTileSize(tif) as usize).max(tile_row_bytes * th as usize)];
    let mut band = vec![0u8; row_bytes * th as usize];
    for s in 0..planes {
        for y in (0..h).step_by(th as usize) {
            let rows = th.min(h - y) as usize;
            for x in (0..w).step_by(tw as usize) {
                let start = x as usize * bits_per_pixel / 8;
                let len = tile_row_bytes.min(row_bytes - start);
                let sparse = crate::ffi::is_strile_sparse(tif, TIFFComputeTile(tif, x, y, 0, s));
                if !sparse && TIFFReadTile(tif, tile_buf.as_mut_ptr() as *mut _, x, y, 0, s) < 0 {
                    return Err(anyhow!(
                        "failed to read tile at ({}, {}) (sample {})",
                        x,
                        y,
                        s
                    ));
                }
                for r in 0..rows {
                    let src = if sparse {
                        &nodata_row[start..][..len]
                    } else {
                        &tile_buf[r * tile_row_bytes..][..len]
                    };
                    band[r * row_bytes + start..][..len].copy_from_slice(src);
                }
            }
            for r in 0..rows {
                hasher.write(&band[r * row_bytes..][..row_bytes]);
            }
        }
    }
    Ok(hasher.finish())
}

fn wipe_command(
    input: Vec<PathBuf>,
    output: Option<PathBuf>,
    level: Option<u32>,
    jobs: Option<usize>,
    verbose: bool,
) -> Result<()> {
    let files = expand_tiff_inputs(&input)?;

    check_batch_output(files.len(), &output)?;

    let m = MultiProgress::new();
    let num_jobs = jobs.unwrap_or_else(num_cpus::get);
    let failed = AtomicUsize::new(0);

    files
        .par_iter()
        .with_max_len(num_jobs)
        .for_each(|file_path| {
            let pb = new_file_progress(
                &m,
                format!(
                    "Wiping {:?}",
                    file_path.file_name().unwrap_or(file_path.as_os_str())
                ),
            );

            let result = resolve_target_output(file_path, &output).and_then(|target_output| {
                wipe_single_file(file_path, &target_output, level, verbose, &pb)
            });
            match result {
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
                    failed.fetch_add(1, Ordering::Relaxed);
                    report_file_error(&m, &pb, file_path, &e);
                }
            }
        });

    batch_result(failed.into_inner(), files.len())
}

fn wipe_single_file(
    input: &Path,
    output: &Path,
    level: Option<u32>,
    verbose: bool,
    pb: &ProgressBar,
) -> Result<(u64, u64)> {
    let original_size = fs::metadata(input)?.len();

    // Get IFD count (also validates the input before anything is created)
    let total_pages = count_tiff_pages(input)?;

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create output directory {:?}", parent))?;
    }

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

        fs::rename(&tmp_path, output)
            .with_context(|| format!("Failed to rename {:?} to {:?}", tmp_path, output))?;
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
    if bps > 8 && !bps.is_multiple_of(8) {
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
    if bps < 8 && (interleaved_spp > 1 || !((w as usize) * (bps as usize)).is_multiple_of(8)) {
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

            // Pass 2 emits exactly w*h*interleaved_spp samples per plane. If
            // pass 1 counted a different number (e.g. a short/truncated tile
            // decode), the synthesizer would silently zero-fill the deficit and
            // corrupt the histogram. Verify the counts match and fail loudly.
            let expected_samples = (w as u64) * (h as u64) * (interleaved_spp as u64);
            if hist.total() != expected_samples {
                return Err(anyhow!(
                    "Histogram sample count mismatch on plane {} (counted {}, expected {}); \
                     refusing to write corrupted output",
                    s,
                    hist.total(),
                    expected_samples
                ));
            }

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

/// Tiled-image layout, shared by the compress and wipe tile readers so the
/// tile-count and tile-index arithmetic lives in one place.
struct TileGeometry {
    tiles_across: u32,
    tiles_down: u32,
    tiles_per_plane: u32,
}

impl TileGeometry {
    fn new(w: u32, h: u32, tile_width: u32, tile_length: u32) -> Self {
        let tiles_across = w.div_ceil(tile_width);
        let tiles_down = h.div_ceil(tile_length);
        TileGeometry {
            tiles_across,
            tiles_down,
            tiles_per_plane: tiles_across * tiles_down,
        }
    }

    /// libtiff tile index for tile `(tile_x, tile_y)` of plane `sample`. For
    /// PLANARCONFIG_SEPARATE (`separate` = true) planes are stored
    /// consecutively; otherwise there is a single interleaved plane.
    fn tile_index(&self, tile_x: u32, tile_y: u32, sample: u32, separate: bool) -> u32 {
        let tile_in_plane = tile_y * self.tiles_across + tile_x;
        if separate {
            sample * self.tiles_per_plane + tile_in_plane
        } else {
            tile_in_plane
        }
    }
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

    let geom = TileGeometry::new(w, h, tile_width, tile_length);

    let mut jobs = Vec::with_capacity(geom.tiles_per_plane as usize);
    for tile_y in 0..geom.tiles_down {
        for tile_x in 0..geom.tiles_across {
            let index = geom.tile_index(tile_x, tile_y, sample as u32, num_planes > 1);
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

/// Run `f` with this thread's read handle on `path` at directory `page`. The
/// handle is cached per thread and reopened when the thread last served a
/// different file or page (rayon threads are shared across files and pages).
fn with_worker_handle<R>(path: &CString, page: u16, f: impl FnOnce(*mut TIFF) -> R) -> Result<R> {
    thread_local! {
        static HANDLE: RefCell<Option<(CString, u16, *mut TIFF)>> = const { RefCell::new(None) };
    }
    HANDLE.with(|cell| {
        let mut cell = cell.borrow_mut();
        let reusable = matches!(&*cell, Some((p, pg, _)) if p == path && *pg == page);
        if !reusable {
            if let Some((_, _, old)) = cell.take() {
                unsafe { TIFFClose(old) };
            }
            let tif = unsafe { open_worker_handle(path, page)? };
            *cell = Some((path.clone(), page, tif));
        }
        Ok(f(cell.as_ref().map(|(_, _, tif)| *tif).unwrap()))
    })
}

/// Open an independent read handle on the source file, positioned at `page`.
/// Used by parallel tile workers (each worker gets its own handle, so there
/// is no cross-thread or cross-file state).
unsafe fn open_worker_handle(c_path: &CString, page: u16) -> Result<*mut TIFF> {
    let tif = TIFFOpen(c_path.as_ptr(), CString::new("r")?.as_ptr());
    if tif.is_null() {
        return Err(anyhow!("Failed to open source TIFF (worker)"));
    }
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
                        tile_buffer_size as isize,
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
    let geom = TileGeometry::new(w, h, tile_width, tile_length);

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

            for tile_x in 0..geom.tiles_across {
                let tile_index =
                    geom.tile_index(tile_x, tile_y as u32, sample as u32, num_planes > 1);

                let read = unsafe {
                    crate::ffi::TIFFReadEncodedTile(
                        tif,
                        tile_index,
                        tile_buf.as_mut_ptr() as *mut _,
                        tile_buffer_size as isize,
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
