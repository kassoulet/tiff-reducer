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
        #[arg(short, long, value_enum, default_value_t = CompressionFormat::Zstd)]
        format: CompressionFormat,

        /// Compression level (Zstd: 1-22 default 19, Deflate/LZMA: 1-9, JPEG/WebP: 1-100)
        #[arg(short, long)]
        level: Option<u32>,

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

        /// Number of parallel jobs for file-level processing (default: number of CPUs)
        #[arg(short, long)]
        jobs: Option<usize>,
    },
    /// Analyze a TIFF file and display metadata
    Analyze {
        /// TIFF file to analyze
        file: PathBuf,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum CompressionFormat {
    Zstd,
    Lzma,
    Lzw,
    Deflate,
    Jpeg,
    Webp,
    Lerc,
    LercDeflate,
    LercZstd,
    JpegXl,
    Uncompressed,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Predictor {
    None,
    Horizontal,
    FloatingPoint,
}

impl std::fmt::Display for Predictor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Predictor::None => write!(f, "None"),
            Predictor::Horizontal => write!(f, "Horizontal"),
            Predictor::FloatingPoint => write!(f, "Float"),
        }
    }
}

impl Predictor {
    #[allow(clippy::wrong_self_convention)]
    fn to_ffi(&self) -> u16 {
        match self {
            Predictor::None => PREDICTOR_NONE,
            Predictor::Horizontal => PREDICTOR_HORIZONTAL,
            Predictor::FloatingPoint => PREDICTOR_FLOATINGPOINT,
        }
    }
}

impl std::fmt::Display for CompressionFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionFormat::Zstd => write!(f, "Zstd"),
            CompressionFormat::Lzma => write!(f, "LZMA"),
            CompressionFormat::Lzw => write!(f, "LZW"),
            CompressionFormat::Deflate => write!(f, "Deflate"),
            CompressionFormat::Jpeg => write!(f, "JPEG"),
            CompressionFormat::Webp => write!(f, "WebP"),
            CompressionFormat::Lerc => write!(f, "LERC"),
            CompressionFormat::LercDeflate => write!(f, "LERC-Deflate"),
            CompressionFormat::LercZstd => write!(f, "LERC-Zstd"),
            CompressionFormat::JpegXl => write!(f, "JPEG-XL"),
            CompressionFormat::Uncompressed => write!(f, "Uncompressed"),
        }
    }
}

impl CompressionFormat {
    #[allow(clippy::wrong_self_convention)]
    fn to_ffi(&self) -> u16 {
        match self {
            CompressionFormat::Zstd => COMPRESSION_ZSTD,
            CompressionFormat::Lzma => COMPRESSION_LZMA,
            CompressionFormat::Lzw => COMPRESSION_LZW,
            CompressionFormat::Deflate => COMPRESSION_ADOBE_DEFLATE,
            CompressionFormat::Jpeg => COMPRESSION_JPEG,
            CompressionFormat::Webp => COMPRESSION_WEBP,
            CompressionFormat::Lerc => COMPRESSION_LERC,
            CompressionFormat::LercDeflate => COMPRESSION_LERC_DEFLATE,
            CompressionFormat::LercZstd => COMPRESSION_LERC_ZSTD,
            CompressionFormat::JpegXl => COMPRESSION_JPEGXL,
            CompressionFormat::Uncompressed => COMPRESSION_NONE,
        }
    }
}

fn main() -> Result<()> {
    // Initialize libgeotiff's extended TIFF tag support at program startup
    // This registers all GeoTIFF tags (33550, 33922, 34735, 34736, 34737) with libtiff
    // Must be called before any TIFF files are opened
    unsafe {
        crate::ffi::XTIFFInitialize();
    }

    env_logger::init();

    // Suppress libtiff warnings for unknown tags (GeoTIFF tags)
    unsafe {
        crate::ffi::suppress_warnings();
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze { file } => analyze_file(&file),
        Commands::Compress {
            input,
            output,
            format,
            level,
            quantize,
            extreme,
            dry_run,
            benchmark,
            jobs,
        } => compress_command(
            input, output, format, level, quantize, extreme, dry_run, benchmark, jobs,
        ),
    }
}

fn compression_name(code: u16) -> &'static str {
    match code {
        COMPRESSION_NONE => "Uncompressed",
        COMPRESSION_LZW => "LZW",
        COMPRESSION_JPEG => "JPEG",
        COMPRESSION_ADOBE_DEFLATE => "Deflate",
        COMPRESSION_LZMA => "LZMA",
        COMPRESSION_ZSTD => "Zstd",
        COMPRESSION_WEBP => "WebP",
        COMPRESSION_LERC => "LERC",
        COMPRESSION_LERC_DEFLATE => "LERC-Deflate",
        COMPRESSION_LERC_ZSTD => "LERC-Zstd",
        COMPRESSION_JPEGXL => "JPEG-XL",
        _ => "Unknown",
    }
}

fn analyze_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("File not found: {:?}", path));
    }

    let c_path = CString::new(path.to_str().ok_or_else(|| anyhow!("Invalid path"))?)?;
    unsafe {
        let tif = TIFFOpen(c_path.as_ptr(), CString::new("r")?.as_ptr());
        if tif.is_null() {
            return Err(anyhow!("Failed to open TIFF"));
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
        println!(
            "Compression: {} ({})",
            compression_name(comp),
            comp
        );

        TIFFClose(tif);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn compress_command(
    input: Vec<PathBuf>,
    output: Option<PathBuf>,
    format: CompressionFormat,
    level: Option<u32>,
    quantize: bool,
    extreme: bool,
    dry_run: bool,
    benchmark: bool,
    jobs: Option<usize>,
) -> Result<()> {
    // Collect all files from input paths (files and directories)
    let files: Vec<PathBuf> = input
        .iter()
        .flat_map(|path| {
            if path.is_dir() {
                fs::read_dir(path)
                    .ok()
                    .map(|entries| {
                        entries
                            .filter_map(|e| e.ok())
                            .map(|e| e.path())
                            .filter(|p| {
                                p.extension()
                                    .is_some_and(|ext| ext == "tif" || ext == "tiff")
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
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
                format,
                level,
                quantize,
                extreme,
                dry_run,
                benchmark,
                &pb,
            ) {
                Ok((original, compressed, best_fmt)) => {
                    pb.set_position(100);
                    pb.finish_with_message("Done");

                    if !extreme {
                        // Display compression result (only if not extreme, since extreme shows its own results)
                        let ratio = if original > 0 {
                            (1.0 - (compressed as f64 / original as f64)) * 100.0
                        } else {
                            0.0
                        };
                        println!(
                            "[{}] {} -> {} bytes ({:.1}% reduction, {})",
                            file_path
                                .file_name()
                                .unwrap_or(file_path.as_os_str())
                                .to_string_lossy(),
                            original,
                            compressed,
                            ratio,
                            best_fmt
                        );
                    } else {
                        // In extreme mode, show final result
                        let ratio = if original > 0 {
                            (1.0 - (compressed as f64 / original as f64)) * 100.0
                        } else {
                            0.0
                        };
                        println!(
                            "\n[{}] Final: {} -> {} bytes ({:.1}% reduction, {})",
                            file_path
                                .file_name()
                                .unwrap_or(file_path.as_os_str())
                                .to_string_lossy(),
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
    format: CompressionFormat,
    level: Option<u32>,
    quantize: bool,
    extreme: bool,
    dry_run: bool,
    benchmark: bool,
    pb: &ProgressBar,
) -> Result<(u64, u64, String)> {
    let original_size = fs::metadata(input)?.len();
    let start_time = std::time::Instant::now();

    let formats = if extreme {
        vec![
            CompressionFormat::Uncompressed,
            CompressionFormat::Zstd,
            CompressionFormat::Lzma,
            CompressionFormat::Deflate,
            CompressionFormat::JpegXl,
        ]
    } else {
        vec![format]
    };

    // Determine sample format to decide which predictors to test
    let sample_format = get_sample_format(input)?;
    let is_float = sample_format == SAMPLEFORMAT_IEEEFP;

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
    } else {
        vec![Predictor::Horizontal] // default
    };

    let mut best_format = formats[0];
    let mut best_predictor = predictors[0];
    let mut best_size = u64::MAX;
    let mut results: Vec<(CompressionFormat, Predictor, u64)> = Vec::new();

    if extreme {
        pb.set_message(format!(
            "Extreme mode: benchmarking formats+predictors for {:?}",
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
            let temp_out = input.with_extension(format!("tmp_{:?}_{}", fmt, i));
            let cid = fmt.to_ffi();
            let pid = pred.to_ffi();
            let _ = run_compression_pass(input, &temp_out, cid, pid, None, quantize);
            let size = temp_out.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
            results.push((*fmt, *pred, size));
            if size < best_size {
                best_size = size;
                best_format = *fmt;
                best_predictor = *pred;
            }
            let _ = fs::remove_file(temp_out);

            // Update progress
            let progress = ((i + 1) as u64 * 100) / total as u64;
            pb.set_position(progress);
            pb.set_message(format!("Extreme: {}/{} combinations tested", i + 1, total));
        }

        // Display results for each combination
        println!(
            "\n[{}] Extreme mode results:",
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
        return Ok((original_size, 0, format!("{best_format}+{best_predictor}")));
    }

    // Final compression with best format and predictor
    let cid = best_format.to_ffi();
    let pid = best_predictor.to_ffi();
    run_compression_pass(input, output, cid, pid, level, quantize)?;

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
    ))
}

/// Get the sample format of a TIFF file
fn get_sample_format(path: &Path) -> Result<u16> {
    let c_path = CString::new(path.to_str().ok_or_else(|| anyhow!("Invalid path"))?)?;
    unsafe {
        let tif = TIFFOpen(c_path.as_ptr(), CString::new("r")?.as_ptr());
        if tif.is_null() {
            return Err(anyhow!("Failed to open TIFF file: {:?}", path));
        }
        let mut fmt: u16 = 0;
        if TIFFGetField(tif, TIFFTAG_SAMPLEFORMAT, &mut fmt) == 0 {
            // Tag not present, default to uint
            fmt = SAMPLEFORMAT_UINT;
        }
        TIFFClose(tif);
        Ok(fmt)
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

        // Register GeoTIFF tags on source for proper reading of GeoTIFF metadata
        crate::metadata::register_geotiff_tags_ffi(tif_src);

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

        // Register GeoTIFF tags on destination for proper writing of GeoTIFF metadata
        // This must be done BEFORE setting any other tags to avoid compression reset
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

    // Handle invalid/missing samples per pixel
    if spp == 0 {
        spp = 1; // Default to 1 sample per pixel
    }

    // Handle missing photometric (default to minisblack for grayscale)
    if photometric == 0 {
        photometric = PHOTOMETRIC_MINISBLACK;
    }

    // Handle missing planar config (default to contiguous)
    if planar == 0 {
        planar = PLANARCONFIG_CONTIG;
    }

    // Check if source is tiled before we start processing
    let is_tiled = crate::ffi::TIFFIsTiled(tif_src) != 0;

    // Set required tags for this IFD (image structure)
    if TIFFSetField(tif_dst, TIFFTAG_IMAGEWIDTH, w) == 0 {
        return Err(anyhow!("Failed to set image width"));
    }
    if TIFFSetField(tif_dst, TIFFTAG_IMAGELENGTH, h) == 0 {
        return Err(anyhow!("Failed to set image length"));
    }

    // Preserve original image parameters
    if TIFFSetField(tif_dst, TIFFTAG_BITSPERSAMPLE, bps as u32) == 0 {
        return Err(anyhow!("Failed to set bits per sample"));
    }
    if TIFFSetField(tif_dst, TIFFTAG_SAMPLESPERPIXEL, spp as u32) == 0 {
        return Err(anyhow!("Failed to set samples per pixel"));
    }
    if fmt != 0 {
        if TIFFSetField(tif_dst, TIFFTAG_SAMPLEFORMAT, fmt as u32) == 0 {
            return Err(anyhow!("Failed to set sample format"));
        }
    }
    if TIFFSetField(tif_dst, TIFFTAG_PHOTOMETRIC, photometric as u32) == 0 {
        return Err(anyhow!("Failed to set photometric interpretation"));
    }
    // Only set PLANARCONFIG if source had it and spp > 1 (multi-sample)
    // For single-sample images, libtiff expects PLANARCONFIG_CONTIG
    if planar != 0 && spp > 1 {
        if TIFFSetField(tif_dst, TIFFTAG_PLANARCONFIG, planar as u32) == 0 {
            return Err(anyhow!("Failed to set planar configuration"));
        }
    }

    // For tiled images, preserve the tiled format; otherwise use strips
    if is_tiled {
        let mut tile_width: u32 = 0;
        let mut tile_length: u32 = 0;
        TIFFGetField(tif_src, TIFFTAG_TILEWIDTH, &mut tile_width);
        TIFFGetField(tif_src, TIFFTAG_TILELENGTH, &mut tile_length);
    }

    if TIFFSetField(tif_dst, TIFFTAG_ROWSPERSTRIP, h) == 0 {
        return Err(anyhow!("Failed to set rows per strip"));
    }

    // Set compression AFTER image structure but BEFORE metadata copying
    // Cast to i32 as required for variadic FFI functions (libtiff expects uint16_vap which is int)
    if TIFFSetField(tif_dst, TIFFTAG_COMPRESSION, compression as i32) == 0 {
        return Err(anyhow!("Failed to set compression codec"));
    }

    // Resolution tags (optional but commonly present)
    let mut xres: f32 = 0.0;
    let mut yres: f32 = 0.0;
    let mut resunit: u16 = 0;
    if TIFFGetField(tif_src, TIFFTAG_XRESOLUTION, &mut xres) != 0 {
        if TIFFSetField(tif_dst, TIFFTAG_XRESOLUTION, xres as f64) == 0 {
            return Err(anyhow!("Failed to set X resolution"));
        }
    }
    if TIFFGetField(tif_src, TIFFTAG_YRESOLUTION, &mut yres) != 0 {
        if TIFFSetField(tif_dst, TIFFTAG_YRESOLUTION, yres as f64) == 0 {
            return Err(anyhow!("Failed to set Y resolution"));
        }
    }
    if TIFFGetField(tif_src, TIFFTAG_RESOLUTIONUNIT, &mut resunit) != 0 {
        if TIFFSetField(tif_dst, TIFFTAG_RESOLUTIONUNIT, resunit as u32) == 0 {
            return Err(anyhow!("Failed to set resolution unit"));
        }
    }

    // Clone metadata from source to destination (GeoTIFF, ICC, alpha, etc.)
    // Only for first page to avoid duplicating file-level metadata
    if is_first_page {
        clone_metadata(tif_src, tif_dst)?;
    }

    // Set compression level for supported codecs with input validation
    if let Some(lvl) = level {
        // Validate compression level to prevent potential issues
        if lvl > 10000 {
            return Err(anyhow!("Compression level too large: {}", lvl));
        }

        match compression {
            COMPRESSION_LZMA => {
                let clamped: i32 = lvl.clamp(1, 9) as i32;
                if TIFFSetField(tif_dst, TIFFTAG_LZMAPRESET, clamped) == 0 {
                    return Err(anyhow!("Failed to set LZMA preset"));
                }
            }
            COMPRESSION_JPEGXL => {
                let clamped: i32 = lvl.clamp(1, 100) as i32;
                if TIFFSetField(tif_dst, TIFFTAG_DEFLATELEVEL, clamped) == 0 {
                    return Err(anyhow!("Failed to set Deflate level"));
                }
            }
            _ => {}
        }
    }

    // Validate predictor for bit depth and sample format
    // Predictors only work with certain compression formats (LZW, Deflate, Zstd, LZMA, etc.)
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
                // Horizontal predictor works with 8, 16, and 32-bit integer samples
                if (bps == 8 || bps == 16 || bps == 32)
                    && (fmt == SAMPLEFORMAT_UINT || fmt == SAMPLEFORMAT_INT)
                {
                    PREDICTOR_HORIZONTAL
                } else {
                    PREDICTOR_NONE
                }
            }
            PREDICTOR_FLOATINGPOINT => {
                // Floating point predictor only works with IEEE floating point samples
                // Supported bit depths: 16 (half), 24, 32 (single), 64 (double)
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

    // Set predictor (compression was already set earlier)
    if final_predictor != PREDICTOR_NONE {
        if TIFFSetField(tif_dst, TIFFTAG_PREDICTOR, final_predictor as u32) == 0 {
            return Err(anyhow!("Failed to set predictor"));
        }
    }

    // Write image data
    if is_tiled {
        process_tiled_image(tif_src, tif_dst, w, h, spp, bps, fmt, quantize)?;
    } else {
        process_striped_image(tif_src, tif_dst, w, h, spp, bps, fmt, quantize)?;
    }

    if TIFFWriteDirectory(tif_dst) == 0 {
        return Err(anyhow!("Failed to write directory"));
    }
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
    // Maximum scanline size to prevent memory exhaustion (1GB limit)
    const MAX_SCANLINE_SIZE: usize = 1024 * 1024 * 1024;

    let in_scanline = TIFFScanlineSize(tif_src) as usize;

    // Validate scanline size to prevent buffer overflow
    if in_scanline == 0 || in_scanline > MAX_SCANLINE_SIZE {
        return Err(anyhow!("Invalid scanline size: {}", in_scanline));
    }

    // Check for multiplication overflow when calculating output scanline
    let out_scanline = if quantize {
        w.checked_mul(_spp as u32)
            .and_then(|s| s.try_into().ok())
            .ok_or_else(|| anyhow!("Invalid image dimensions - overflow"))?
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
                let actual_samples = (in_scanline / 4).min((w * _spp as u32) as usize);
                let slice_f32 =
                    std::slice::from_raw_parts(buf_in.as_ptr() as *const f32, actual_samples);
                crate::quantize::quantize_f32_to_u8(slice_f32, &mut buf_out);
            } else if bps == 16 && fmt == SAMPLEFORMAT_INT {
                let actual_samples = (in_scanline / 2).min((w * _spp as u32) as usize);
                let slice_i16 =
                    std::slice::from_raw_parts(buf_in.as_ptr() as *const i16, actual_samples);
                crate::quantize::quantize_i16_to_u8(slice_i16, &mut buf_out);
            } else {
                let take = buf_in.len().min(buf_out.len());
                buf_out[..take].copy_from_slice(&buf_in[..take]);
            }
            if TIFFWriteScanline(tif_dst, buf_out.as_ptr() as *mut _, row, 0) < 0 {
                return Err(anyhow!("Failed to write scanline {}", row));
            }
        } else if TIFFWriteScanline(tif_dst, buf_in.as_ptr() as *mut _, row, 0) < 0 {
            return Err(anyhow!("Failed to write scanline {}", row));
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
    _fmt: u16,
    _quantize: bool,
) -> Result<()> {
    // For tiled images, we use libtiff's built-in tile reading
    // and convert to scanlines for writing

    // Maximum image size to prevent memory exhaustion (4GB limit)
    const MAX_IMAGE_SIZE: usize = 4 * 1024 * 1024 * 1024;

    // Get tile dimensions
    let mut tile_width: u32 = 0;
    let mut tile_length: u32 = 0;
    if TIFFGetField(tif_src, TIFFTAG_TILEWIDTH, &mut tile_width) == 0
        || TIFFGetField(tif_src, TIFFTAG_TILELENGTH, &mut tile_length) == 0
    {
        return Err(anyhow!("Failed to read tile dimensions"));
    }

    // Calculate bytes per pixel and row size with overflow checking
    let bytes_per_sample = (bps as usize).div_ceil(8);
    let bytes_per_pixel = bytes_per_sample
        .checked_mul(spp as usize)
        .ok_or_else(|| anyhow!("Bytes per pixel overflow"))?;
    let row_size = (w as usize)
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| anyhow!("Row size overflow"))?;

    // Check total image size
    let total_size = row_size
        .checked_mul(h as usize)
        .ok_or_else(|| anyhow!("Image size overflow"))?;
    if total_size > MAX_IMAGE_SIZE {
        return Err(anyhow!("Image too large: {} bytes", total_size));
    }

    // Create output buffer for all scanlines
    let mut image_data = vec![0u8; total_size];

    // Calculate number of tiles with overflow checking
    let tiles_across = w.div_ceil(tile_width);
    let tiles_down = h.div_ceil(tile_length);

    // Get tile size for buffer allocation with overflow checking
    let tile_buffer_size = (tile_width as usize)
        .checked_mul(tile_length as usize)
        .and_then(|s| s.checked_mul(bytes_per_pixel))
        .ok_or_else(|| anyhow!("Tile buffer size overflow"))?;
    let mut tile_buf = vec![0u8; tile_buffer_size];

    // Read each tile and place in output buffer
    for tile_y in 0..tiles_down {
        for tile_x in 0..tiles_across {
            let tile_index = tile_y * tiles_across + tile_x;

            // Read encoded tile (automatically decompresses)
            let read_size = crate::ffi::TIFFReadEncodedTile(
                tif_src,
                tile_index,
                tile_buf.as_mut_ptr() as *mut _,
                tile_buffer_size as u32,
            );

            if read_size < 0 {
                return Err(anyhow!(
                    "Failed to decode tile {} at ({}, {})",
                    tile_index,
                    tile_x,
                    tile_y
                ));
            }

            // Calculate tile position in image with overflow checking
            let start_x = (tile_x as usize)
                .checked_mul(tile_width as usize)
                .ok_or_else(|| anyhow!("Tile position overflow"))?;
            let start_y = (tile_y as usize)
                .checked_mul(tile_length as usize)
                .ok_or_else(|| anyhow!("Tile position overflow"))?;

            // Calculate actual tile dimensions (edge tiles may be smaller)
            let actual_width = std::cmp::min(tile_width as usize, w as usize - start_x);
            let actual_height = std::cmp::min(tile_length as usize, h as usize - start_y);

            // Copy tile data to image buffer row by row with bounds checking
            let src_row_size = actual_width
                .checked_mul(bytes_per_pixel)
                .ok_or_else(|| anyhow!("Source row size overflow"))?;
            for row in 0..actual_height {
                let src_start = row
                    .checked_mul(src_row_size)
                    .ok_or_else(|| anyhow!("Source start overflow"))?;
                if src_start >= tile_buf.len() {
                    continue; // Skip if source is out of bounds
                }

                let dst_start = (start_y
                    .checked_add(row)
                    .ok_or_else(|| anyhow!("Destination start overflow"))?)
                .checked_mul(row_size)
                .and_then(|s| s.checked_add(start_x.checked_mul(bytes_per_pixel)?))
                .ok_or_else(|| anyhow!("Destination start overflow"))?;

                let remaining_buf = tile_buf
                    .len()
                    .checked_sub(src_start)
                    .ok_or_else(|| anyhow!("Buffer underflow"))?;
                let copy_len = src_row_size.min(remaining_buf);

                if let Some(end) = dst_start.checked_add(copy_len) {
                    if end <= image_data.len() {
                        image_data[dst_start..end]
                            .copy_from_slice(&tile_buf[src_start..src_start + copy_len]);
                    }
                }
            }
        }
    }

    // Write all scanlines to destination
    for row in 0..h {
        let row_start = (row as usize) * row_size;
        if TIFFWriteScanline(tif_dst, image_data[row_start..].as_ptr() as *mut _, row, 0) < 0 {
            return Err(anyhow!("Failed to write scanline {}", row));
        }
    }

    Ok(())
}
