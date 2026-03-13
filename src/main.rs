mod ffi;
mod metadata;
mod quantize;

use crate::ffi::*;
use crate::metadata::clone_metadata;
use clap::{Parser, Subcommand, ValueEnum};
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Result, anyhow};
use rayon::prelude::*;

#[derive(Parser)]
#[command(name = "tiffthin-rs")]
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
        }
    }
}

impl CompressionFormat {
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
        Commands::Compress { input, output, format, level, quantize, extreme, dry_run, benchmark } => {
            compress_command(input, output, format, level, quantize, extreme, dry_run, benchmark)
        }
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
        println!("Format: {}", match fmt {
            SAMPLEFORMAT_UINT => "Unsigned Integer",
            SAMPLEFORMAT_INT => "Signed Integer",
            SAMPLEFORMAT_IEEEFP => "Floating Point",
            _ => "Unknown",
        });
        println!("Compression Codec Code: {}", comp);

        TIFFClose(tif);
    }
    Ok(())
}

fn compress_command(input: PathBuf, output: Option<PathBuf>, format: CompressionFormat, level: Option<u32>, quantize: bool, extreme: bool, dry_run: bool, benchmark: bool) -> Result<()> {
    // Set default compression level for Zstd if not specified (libtiff 4.7+)
    let level = level.or_else(|| {
        if format == CompressionFormat::Zstd {
            Some(19) // Default Zstd level for better compression
        } else {
            None
        }
    });

    let files = if input.is_dir() {
        fs::read_dir(&input)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |ext| ext == "tif" || ext == "tiff"))
            .collect::<Vec<_>>()
    } else {
        vec![input]
    };

    let m = MultiProgress::new();

    files.par_iter().for_each(|file_path| {
        let pb = m.add(ProgressBar::new(100));
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] {msg} [{bar:40.cyan/blue}] {pos}%")
            .unwrap());
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

        match process_single_file(file_path, &target_output, format, level, quantize, extreme, dry_run, benchmark, &pb) {
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
                    println!("[{}] {} -> {} bytes ({:.1}% reduction, {})",
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
                    println!("\n[{}] Final: {} -> {} bytes ({:.1}% reduction, {})",
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

fn process_single_file(input: &Path, output: &Path, format: CompressionFormat, level: Option<u32>, quantize: bool, extreme: bool, dry_run: bool, benchmark: bool, pb: &ProgressBar) -> Result<(u64, u64, String)> {
    let original_size = fs::metadata(input)?.len();
    let start_time = std::time::Instant::now();

    let formats = if extreme {
        vec![
            CompressionFormat::Zstd,
            CompressionFormat::Lzma,
            CompressionFormat::Deflate,
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
            vec![Predictor::None, Predictor::Horizontal, Predictor::FloatingPoint]
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
        pb.set_message(format!("Extreme mode: benchmarking formats+predictors for {:?}", input.file_name().unwrap()));
        
        let mut combinations = Vec::new();
        for &fmt in &formats {
            for &pred in &predictors {
                // Skip predictors for lossy compression (JPEG, WebP)
                if matches!(fmt, CompressionFormat::Jpeg | CompressionFormat::Webp) && pred != Predictor::None {
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
        println!("\n[{}] Extreme mode results:", input.file_name().unwrap().to_string_lossy());
        for (fmt, pred, size) in &results {
            let ratio = if original_size > 0 {
                (1.0 - (*size as f64 / original_size as f64)) * 100.0
            } else {
                0.0
            };
            let marker = if *fmt == best_format && *pred == best_predictor { "✓" } else { " " };
            println!("  [{}] {:<10} {:<10} {} bytes ({:.1}% reduction)",
                marker, fmt, pred, size, ratio);
        }
        pb.set_message(format!("Winner: {} + {} ({} bytes)", best_format, best_predictor, best_size));
    } else {
        pb.set_message(format!("Compressing {:?}", input.file_name().unwrap()));
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
        println!("\n[{}] Benchmark Results:", input.file_name().unwrap().to_string_lossy());
        println!("  Original size:   {} bytes", original_size);
        println!("  Compressed size: {} bytes", compressed_size);
        println!("  Compression:     {:.1}% reduction", ratio);
        println!("  Time elapsed:    {:.3}s", elapsed.as_secs_f64());
        println!("  Throughput:      {:.2} MB/s", throughput_mbs);
    }

    Ok((original_size, compressed_size, format!("{best_format}+{best_predictor}")))
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

fn run_compression_pass(input: &Path, output: &Path, compression: u16, predictor: u16, level: Option<u32>, quantize: bool) -> Result<()> {
    let c_input = CString::new(input.to_str().ok_or_else(|| anyhow!("Invalid input path"))?)?;

    // Read GeoTIFF tags from the raw file before libtiff processing
    // Note: GeoTIFF tags are only read from the first IFD
    let geotiff_data = crate::metadata::read_geotiff_from_file(input)
        .map_err(|e| anyhow!("Failed to read GeoTIFF data: {}", e))?;

    unsafe {
        let tif_src = TIFFOpen(c_input.as_ptr(), CString::new("r")?.as_ptr());
        if tif_src.is_null() { return Err(anyhow!("Failed to open source TIFF")); }

        // Register GeoTIFF tags immediately after opening
        // This must happen before any directory operations
        crate::metadata::register_geotiff_tags_ffi(tif_src);

        let tmp_path = output.with_extension("tmp_tiffthin");
        let c_tmp = CString::new(tmp_path.to_str().ok_or_else(|| anyhow!("Invalid temp path"))?)?;

        let mode_str = if input.metadata()?.len() > 4 * 1024 * 1024 * 1024 { "w8" } else { "w" };
        let tif_dst = TIFFOpen(c_tmp.as_ptr(), CString::new(mode_str)?.as_ptr());
        if tif_dst.is_null() {
            TIFFClose(tif_src);
            return Err(anyhow!("Failed to open destination TIFF"));
        }

        // Register GeoTIFF tags on destination as well
        crate::metadata::register_geotiff_tags_ffi(tif_dst);

        // Process all pages/IFDs
        // Note: We're already positioned at directory 0 after opening
        let mut page = 0;
        loop {
            // Process current IFD
            process_single_ifd(tif_src, tif_dst, compression, predictor, level, quantize, &geotiff_data, page == 0)?;

            // Try to read next directory
            if TIFFReadDirectory(tif_src) == 0 {
                break;  // No more pages
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
unsafe fn process_single_ifd(
    tif_src: *mut TIFF,
    tif_dst: *mut TIFF,
    compression: u16,
    predictor: u16,
    level: Option<u32>,
    quantize: bool,
    geotiff_data: &crate::metadata::GeoTiffData,
    is_first_page: bool,
) -> Result<()> {
    // Get image dimensions for this IFD
    let mut w = 0u32;
    let mut h = 0u32;
    TIFFGetField(tif_src, TIFFTAG_IMAGEWIDTH, &mut w);
    TIFFGetField(tif_src, TIFFTAG_IMAGELENGTH, &mut h);

    // Clone metadata (only for first page - GeoTIFF tags are file-level)
    if is_first_page {
        clone_metadata(tif_src, tif_dst, geotiff_data);
    }

    let mut bps = 0u16;
    let mut spp = 0u16;
    let mut fmt = 0u16;
    TIFFGetField(tif_src, TIFFTAG_BITSPERSAMPLE, &mut bps);
    TIFFGetField(tif_src, TIFFTAG_SAMPLESPERPIXEL, &mut spp);
    TIFFGetField(tif_src, TIFFTAG_SAMPLEFORMAT, &mut fmt);

    // Copy basic image tags for this IFD
    TIFFSetField(tif_dst, TIFFTAG_IMAGEWIDTH, w);
    TIFFSetField(tif_dst, TIFFTAG_IMAGELENGTH, h);
    TIFFSetField(tif_dst, TIFFTAG_BITSPERSAMPLE, bps as u32);
    TIFFSetField(tif_dst, TIFFTAG_SAMPLESPERPIXEL, spp as u32);
    // Only set SampleFormat if successfully read (non-zero)
    if fmt != 0 {
        TIFFSetField(tif_dst, TIFFTAG_SAMPLEFORMAT, fmt as u32);
    }

    // Copy photometric interpretation
    let mut photometric: u16 = 0;
    if TIFFGetField(tif_src, TIFFTAG_PHOTOMETRIC, &mut photometric) != 0 {
        TIFFSetField(tif_dst, TIFFTAG_PHOTOMETRIC, photometric as u32);
    }

    // Copy planar config
    let mut planar: u16 = 0;
    if TIFFGetField(tif_src, TIFFTAG_PLANARCONFIG, &mut planar) != 0 {
        TIFFSetField(tif_dst, TIFFTAG_PLANARCONFIG, planar as u32);
    }

    // Copy resolution tags
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

    // Copy colormap if present (for palette images)
    crate::metadata::copy_colormap(tif_src, tif_dst);

    // Copy ExtraSamples if present (for alpha channels)
    crate::metadata::copy_extrasamples(tif_src, tif_dst);

    // Copy ICC color profile if present
    crate::metadata::copy_icc_profile(tif_src, tif_dst);

    // Copy YCbCr color space tags if present
    crate::metadata::copy_ycbcr_tags(tif_src, tif_dst);

    // Copy CMYK/Ink-related tags if present
    crate::metadata::copy_cmyk_tags(tif_src, tif_dst);

    // Copy ImageDescription tag (for OME-XML metadata)
    crate::metadata::copy_image_description(tif_src, tif_dst);

    if quantize {
        TIFFSetField(tif_dst, TIFFTAG_BITSPERSAMPLE, 8u32);
        TIFFSetField(tif_dst, TIFFTAG_SAMPLEFORMAT, SAMPLEFORMAT_UINT as u32);
    }

    // Set compression first, then compression level (libtiff 4.7+)
    TIFFSetField(tif_dst, TIFFTAG_COMPRESSION, compression as u32);
    TIFFSetField(tif_dst, TIFFTAG_PREDICTOR, predictor as u32);

    // Set compression level after compression is set (codec-specific)
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
            _ => {}
        }
    }

    // Check if source is tiled
    let is_tiled = TIFFIsTiled(tif_src) != 0;

    if is_tiled {
        // Handle tiled images
        process_tiled_image(tif_src, tif_dst, w, h, spp, bps, fmt, quantize)?;
    } else {
        // Handle strip-based images
        process_striped_image(tif_src, tif_dst, w, h, spp, bps, fmt, quantize)?;
    }

    // Write this IFD to disk
    TIFFWriteDirectory(tif_dst);

    Ok(())
}

/// Process a strip-based TIFF image
/// Note: Parallelism is handled at the file level (multiple files processed in parallel)
/// Per-file parallelism would require separate TIFF handles per thread
unsafe fn process_striped_image(
    tif_src: *mut TIFF,
    tif_dst: *mut TIFF,
    w: u32,
    h: u32,
    spp: u16,
    bps: u16,
    fmt: u16,
    quantize: bool,
) -> Result<()> {
    let in_scanline = TIFFScanlineSize(tif_src);
    let out_scanline = if quantize { w * spp as u32 } else { in_scanline };

    let mut buf_in = vec![0u8; in_scanline as usize];
    let mut buf_out = vec![0u8; out_scanline as usize];

    for row in 0..h {
        TIFFReadScanline(tif_src, buf_in.as_mut_ptr() as *mut _, row, 0);

        if quantize {
            if bps == 32 && fmt == SAMPLEFORMAT_IEEEFP {
                let slice_f32 = std::slice::from_raw_parts(buf_in.as_ptr() as *const f32, (w * spp as u32) as usize);
                crate::quantize::quantize_f32_to_u8(slice_f32, &mut buf_out);
            } else if bps == 16 && fmt == SAMPLEFORMAT_INT {
                let slice_i16 = std::slice::from_raw_parts(buf_in.as_ptr() as *const i16, (w * spp as u32) as usize);
                crate::quantize::quantize_i16_to_u8(slice_i16, &mut buf_out);
            } else {
                let take = std::cmp::min(buf_in.len(), buf_out.len());
                buf_out[..take].copy_from_slice(&buf_in[..take]);
            }
            TIFFWriteScanline(tif_dst, buf_out.as_ptr() as *mut _, row, 0);
        } else {
            TIFFWriteScanline(tif_dst, buf_in.as_ptr() as *mut _, row, 0);
        }
    }

    Ok(())
}

/// Process a tiled TIFF image
/// Note: Parallelism is handled at the file level (multiple files processed in parallel)
/// Per-file parallelism would require separate TIFF handles per thread
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
    // Get tile dimensions from source
    let mut tile_width: u32 = 0;
    let mut tile_length: u32 = 0;
    TIFFGetField(tif_src, TIFFTAG_TILEWIDTH, &mut tile_width);
    TIFFGetField(tif_src, TIFFTAG_TILELENGTH, &mut tile_length);

    if tile_width == 0 || tile_length == 0 {
        return Err(anyhow!("Invalid tile dimensions"));
    }

    // Set tile dimensions on destination (same as source)
    TIFFSetField(tif_dst, TIFFTAG_TILEWIDTH, tile_width);
    TIFFSetField(tif_dst, TIFFTAG_TILELENGTH, tile_length);

    // Get source tile size for reading
    let src_tile_size = TIFFTileSize(tif_src) as usize;
    let max_tile_size = src_tile_size * 2;
    let num_tiles = TIFFNumberOfTiles(tif_src);

    let mut buf_in = vec![0u8; src_tile_size];
    let mut buf_out = vec![0u8; max_tile_size];

    // Process each tile sequentially
    for tile in 0..num_tiles {
        let bytes_read = TIFFReadEncodedTile(tif_src, tile, buf_in.as_mut_ptr() as *mut _, src_tile_size as u32);
        if bytes_read < 0 {
            eprintln!("Warning: Failed to read tile {}", tile);
            continue;
        }

        if quantize {
            let bytes_per_pixel = if bps == 32 { 4 } else if bps == 16 { 2 } else { 1 };
            let actual_pixels = (bytes_read as usize) / bytes_per_pixel;

            if bps == 32 && fmt == SAMPLEFORMAT_IEEEFP {
                let slice_f32 = std::slice::from_raw_parts(buf_in.as_ptr() as *const f32, actual_pixels);
                crate::quantize::quantize_f32_to_u8(slice_f32, &mut buf_out);
            } else if bps == 16 && fmt == SAMPLEFORMAT_INT {
                let slice_i16 = std::slice::from_raw_parts(buf_in.as_ptr() as *const i16, actual_pixels);
                crate::quantize::quantize_i16_to_u8(slice_i16, &mut buf_out);
            } else {
                buf_out[..bytes_read as usize].copy_from_slice(&buf_in[..bytes_read as usize]);
            }
            let out_size = actual_pixels;
            TIFFWriteEncodedTile(tif_dst, tile, buf_out.as_ptr() as *mut _, out_size as u32);
        } else {
            TIFFWriteEncodedTile(tif_dst, tile, buf_in.as_ptr() as *mut _, bytes_read as u32);
        }
    }

    Ok(())
}
