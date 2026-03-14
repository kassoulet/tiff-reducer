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
use std::path::{Path, PathBuf};

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
        /// Input file or directory
        input: PathBuf,

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
        }
    }
}

fn main() -> Result<()> {
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
        let mut fmt = 0u16;

        TIFFGetField(tif, TIFFTAG_IMAGEWIDTH, &mut w);
        TIFFGetField(tif, TIFFTAG_IMAGELENGTH, &mut h);
        TIFFGetField(tif, TIFFTAG_BITSPERSAMPLE, &mut bps);
        TIFFGetField(tif, TIFFTAG_SAMPLESPERPIXEL, &mut spp);
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
        println!("Compression Codec Code: {}", comp);

        TIFFClose(tif);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn compress_command(
    input: PathBuf,
    output: Option<PathBuf>,
    format: CompressionFormat,
    level: Option<u32>,
    quantize: bool,
    extreme: bool,
    dry_run: bool,
    benchmark: bool,
    jobs: Option<usize>,
) -> Result<()> {
    let files = if input.is_dir() {
        fs::read_dir(&input)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|ext| ext == "tif" || ext == "tiff")
            })
            .collect::<Vec<_>>()
    } else {
        vec![input]
    };

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
                    .template("{spinner:.green} [{elapsed_precise}] {msg} [{bar:40.cyan/blue}] {pos}%")
                    .unwrap(),
            );
            pb.set_position(0);
            pb.set_message(format!("Processing {:?}", file_path.file_name().unwrap()));

            let target_output = if let Some(ref out) = output {
                if out.is_dir() {
                    out.join(file_path.file_name().unwrap())
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
                            file_path.file_name().unwrap().to_string_lossy(),
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
                            file_path.file_name().unwrap().to_string_lossy(),
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
            input.file_name().unwrap()
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
            input.file_name().unwrap().to_string_lossy()
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
        pb.set_message(format!("Compressing {:?}", input.file_name().unwrap()));
    }

    if dry_run {
        return Ok((
            original_size,
            0,
            format!("{best_format}+{best_predictor}"),
        ));
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
            input.file_name().unwrap().to_string_lossy()
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
            return Ok(SAMPLEFORMAT_UINT);
        }
        let mut fmt: u16 = 0;
        TIFFGetField(tif, TIFFTAG_SAMPLEFORMAT, &mut fmt);
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

        // Register GeoTIFF tags immediately after opening
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

        // Register GeoTIFF tags on destination
        crate::metadata::register_geotiff_tags_ffi(tif_dst);

        // Process all pages/IFDs
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
    predictor: u16,
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
    let has_photometric = TIFFGetField(tif_src, TIFFTAG_PHOTOMETRIC, &mut photometric) != 0;
    let has_planar = TIFFGetField(tif_src, TIFFTAG_PLANARCONFIG, &mut planar) != 0;

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

    // Skip metadata cloning for now to avoid tag conflicts
    // We'll set all required tags individually
    // if is_first_page {
    //     clone_metadata(tif_src, tif_dst);
    // }

    // Set required tags for this IFD (override cloned values for current page)
    TIFFSetField(tif_dst, TIFFTAG_IMAGEWIDTH, w);
    TIFFSetField(tif_dst, TIFFTAG_IMAGELENGTH, h);
    
    // Apply quantization before setting bits per sample
    let final_bps = if quantize { 8 } else { bps };
    let final_fmt = if quantize { SAMPLEFORMAT_UINT } else { fmt };

    TIFFSetField(tif_dst, TIFFTAG_BITSPERSAMPLE, final_bps as u32);
    TIFFSetField(tif_dst, TIFFTAG_SAMPLESPERPIXEL, spp as u32);
    if final_fmt != 0 {
        TIFFSetField(tif_dst, TIFFTAG_SAMPLEFORMAT, final_fmt as u32);
    }
    TIFFSetField(tif_dst, TIFFTAG_PHOTOMETRIC, photometric as u32);
    // Only set PLANARCONFIG if source had it
    if planar != 0 {
        TIFFSetField(tif_dst, TIFFTAG_PLANARCONFIG, planar as u32);
    }
    
    // Note: We don't set RowsPerStrip explicitly - libtiff will calculate it automatically

    // Resolution tags (optional but commonly present)
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

    TIFFSetField(tif_dst, TIFFTAG_COMPRESSION, compression as u32);
    
    // Set compression level immediately after compression codec
    if let Some(lvl) = level {
        match compression {
            COMPRESSION_ZSTD => {
                let clamped: i32 = lvl.clamp(1, 22) as i32;
                TIFFSetField(tif_dst, TIFFTAG_ZSTD_LEVEL, clamped);
            }
            COMPRESSION_ADOBE_DEFLATE | COMPRESSION_LZW => {
                let clamped: i32 = lvl.clamp(1, 9) as i32;
                TIFFSetField(tif_dst, TIFFTAG_DEFLATELEVEL, clamped);
            }
            COMPRESSION_LZMA => {
                let clamped: i32 = lvl.clamp(1, 9) as i32;
                TIFFSetField(tif_dst, TIFFTAG_LZMAPRESET, clamped);
            }
            COMPRESSION_JPEGXL => {
                let clamped: i32 = lvl.clamp(1, 100) as i32;
                TIFFSetField(tif_dst, TIFFTAG_DEFLATELEVEL, clamped);
            }
            _ => {}
        }
    }

    // Set other tags
    
    // Validate predictor for bit depth (after compression level is set)
    // Horizontal predictor only works with 8, 16, and 32-bit samples
    // Floating point predictor only works with 32-bit float samples
    // For multi-channel images (spp > 1), only use predictor if bit depth is standard
    let valid_predictor = match predictor {
        PREDICTOR_HORIZONTAL => {
            // Only 8, 16, and 32-bit integer samples support horizontal predictor
            // For multi-channel images, be more conservative
            if spp > 1 {
                // For RGB/CMYK, only use predictor for standard bit depths
                bps == 8 || bps == 16
            } else {
                bps == 8 || bps == 16 || bps == 32
            }
        }
        PREDICTOR_FLOATINGPOINT => {
            // Floating point predictor requires 32-bit float samples
            bps == 32 && fmt == SAMPLEFORMAT_IEEEFP
        }
        _ => true, // PREDICTOR_NONE always valid
    };

    let final_predictor = if valid_predictor { predictor } else { PREDICTOR_NONE };
    TIFFSetField(tif_dst, TIFFTAG_PREDICTOR, final_predictor as u32);

    // Use the is_tiled variable defined earlier in the function
    if is_tiled {
        process_tiled_image(tif_src, tif_dst, w, h, spp, bps, fmt, quantize)?;
    } else {
        process_striped_image(tif_src, tif_dst, w, h, spp, bps, fmt, quantize)?;
    }

    TIFFWriteDirectory(tif_dst);
    Ok(())
}

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
    let in_scanline = TIFFScanlineSize(tif_src) as usize;
    let out_scanline = if quantize {
        (w * _spp as u32) as usize
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
                let slice_f32 = std::slice::from_raw_parts(
                    buf_in.as_ptr() as *const f32,
                    actual_samples,
                );
                crate::quantize::quantize_f32_to_u8(slice_f32, &mut buf_out);
            } else if bps == 16 && fmt == SAMPLEFORMAT_INT {
                let actual_samples = (in_scanline / 2).min((w * _spp as u32) as usize);
                let slice_i16 = std::slice::from_raw_parts(
                    buf_in.as_ptr() as *const i16,
                    actual_samples,
                );
                crate::quantize::quantize_i16_to_u8(slice_i16, &mut buf_out);
            } else {
                let take = buf_in.len().min(buf_out.len());
                buf_out[..take].copy_from_slice(&buf_in[..take]);
            }
            if TIFFWriteScanline(tif_dst, buf_out.as_ptr() as *mut _, row, 0) < 0 {
                return Err(anyhow!("Failed to write scanline {}", row));
            }
        } else {
            if TIFFWriteScanline(tif_dst, buf_in.as_ptr() as *mut _, row, 0) < 0 {
                return Err(anyhow!("Failed to write scanline {}", row));
            }
        }
    }
    Ok(())
}

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
    // Tiled image support is currently limited
    // For now, we'll read tiles and write as scanlines using libtiff's automatic conversion
    
    // Get the scanline size
    let in_scanline = TIFFScanlineSize(tif_src) as usize;
    let out_scanline = if quantize {
        (w * spp as u32) as usize
    } else {
        in_scanline
    };

    // Read all scanlines first into a buffer
    // libtiff will handle the tile-to-scanline conversion automatically
    let mut all_scanlines = vec![0u8; in_scanline * h as usize];
    for row in 0..h {
        let offset = (row as usize) * in_scanline;
        if TIFFReadScanline(tif_src, all_scanlines[offset..].as_mut_ptr() as *mut _, row, 0) < 0 {
            return Err(anyhow!("Failed to read scanline {}", row));
        }
    }

    // Process scanlines (quantize if needed)
    let processed_data = if quantize {
        let mut out_buf = vec![0u8; out_scanline * h as usize];
        if bps == 32 && fmt == SAMPLEFORMAT_IEEEFP {
            let actual_samples = (in_scanline / 4).min((w * spp as u32) as usize);
            for row in 0..h as usize {
                let in_offset = row * in_scanline;
                let out_offset = row * out_scanline;
                let slice_f32 = std::slice::from_raw_parts(
                    all_scanlines[in_offset..].as_ptr() as *const f32,
                    actual_samples,
                );
                crate::quantize::quantize_f32_to_u8(slice_f32, &mut out_buf[out_offset..]);
            }
        } else if bps == 16 && fmt == SAMPLEFORMAT_INT {
            let actual_samples = (in_scanline / 2).min((w * spp as u32) as usize);
            for row in 0..h as usize {
                let in_offset = row * in_scanline;
                let out_offset = row * out_scanline;
                let slice_i16 = std::slice::from_raw_parts(
                    all_scanlines[in_offset..].as_ptr() as *const i16,
                    actual_samples,
                );
                crate::quantize::quantize_i16_to_u8(slice_i16, &mut out_buf[out_offset..]);
            }
        } else {
            // Simple copy for other formats
            let take = all_scanlines.len().min(out_buf.len());
            out_buf[..take].copy_from_slice(&all_scanlines[..take]);
        }
        out_buf
    } else {
        all_scanlines
    };

    // Write all scanlines
    for row in 0..h {
        let offset = (row as usize) * out_scanline;
        TIFFWriteScanline(tif_dst, processed_data[offset..].as_ptr() as *mut _, row, 0);
    }
    Ok(())
}

/// Wrapper for TIFFReadTile that handles the varargs properly
unsafe fn libtiff_read_tile(
    tif: *mut TIFF,
    x: u32,
    y: u32,
    z: u16,
    s: u16,
    buf: *mut libc::c_void,
    size: u32,
) -> i32 {
    // libtiff's TIFFReadTile signature:
    // int TIFFReadTile(TIFF* tif, uint32_t x, uint32_t y, uint16_t z, uint16_t s, void* buf, tmsize_t size)
    crate::ffi::TIFFReadTile(tif, x, y, z, s, buf, size)
}
