#!/usr/bin/env python3
"""
Visual Regression Test for tiff-reducer
Calculates PSNR and SSIM metrics between original and compressed TIFF files
"""

import sys
import os
import subprocess
import tempfile
from pathlib import Path

try:
    from osgeo import gdal, osr
    import numpy as np
except ImportError:
    print("GDAL Python bindings required. Install with: pip install gdal numpy")
    sys.exit(1)


def calculate_psnr(original: np.ndarray, compressed: np.ndarray) -> float:
    """
    Calculate Peak Signal-to-Noise Ratio between two arrays.
    PSNR = 10 * log10(MAX^2 / MSE)
    Higher PSNR = better quality (typically >40 dB is excellent)
    """
    mse = np.mean((original.astype(float) - compressed.astype(float)) ** 2)
    if mse == 0:
        return float('inf')  # Perfect match
    
    max_pixel = np.iinfo(original.dtype).max if np.issubdtype(original.dtype, np.integer) else 1.0
    psnr = 10 * np.log10((max_pixel ** 2) / mse)
    return psnr


def calculate_ssim(original: np.ndarray, compressed: np.ndarray) -> float:
    """
    Calculate Structural Similarity Index between two arrays.
    SSIM ranges from -1 to 1, where 1 means identical.
    Simplified implementation without external dependencies.
    """
    C1 = (0.01 * 255) ** 2
    C2 = (0.03 * 255) ** 2
    
    original = original.astype(float)
    compressed = compressed.astype(float)
    
    mu1 = np.mean(original)
    mu2 = np.mean(compressed)
    
    sigma1_sq = np.var(original)
    sigma2_sq = np.var(compressed)
    sigma12 = np.cov(original.flatten(), compressed.flatten())[0, 1]
    
    ssim = ((2 * mu1 * mu2 + C1) * (2 * sigma12 + C2)) / \
           ((mu1 ** 2 + mu2 ** 2 + C1) * (sigma1_sq + sigma2_sq + C2))
    
    return ssim


def read_band_as_array(dataset: gdal.Dataset, band: int = 1) -> np.ndarray:
    """Read a GDAL band as a numpy array."""
    band_obj = dataset.GetRasterBand(band)
    return band_obj.ReadAsArray()


def compare_tiffs(original_path: str, compressed_path: str) -> dict:
    """
    Compare two TIFF files and return metrics.
    """
    orig_ds = gdal.Open(original_path)
    comp_ds = gdal.Open(compressed_path)
    
    if not orig_ds or not comp_ds:
        return {"error": "Failed to open files"}
    
    results = {
        "dimensions_match": True,
        "bands": 0,
        "psnr_per_band": [],
        "ssim_per_band": [],
        "overall_psnr": 0,
        "overall_ssim": 0,
    }
    
    # Check dimensions
    if (orig_ds.RasterXSize != comp_ds.RasterXSize or 
        orig_ds.RasterYSize != comp_ds.RasterYSize):
        results["dimensions_match"] = False
        return results
    
    bands = min(orig_ds.RasterCount, comp_ds.RasterCount)
    results["bands"] = bands
    
    all_psnr = []
    all_ssim = []
    
    for b in range(1, bands + 1):
        orig_arr = read_band_as_array(orig_ds, b)
        comp_arr = read_band_as_array(comp_ds, b)
        
        # Skip if shapes don't match (shouldn't happen after dimension check)
        if orig_arr.shape != comp_arr.shape:
            continue
        
        psnr = calculate_psnr(orig_arr, comp_arr)
        ssim = calculate_ssim(orig_arr, comp_arr)
        
        results["psnr_per_band"].append(psnr)
        results["ssim_per_band"].append(ssim)
        
        if psnr != float('inf'):
            all_psnr.append(psnr)
        all_ssim.append(ssim)
    
    # Calculate overall metrics
    if all_psnr:
        results["overall_psnr"] = np.mean(all_psnr)
    else:
        results["overall_psnr"] = float('inf')
    
    results["overall_ssim"] = np.mean(all_ssim) if all_ssim else 0
    
    return results


def compress_file(input_path: str, output_path: str, tiffthin_path: str) -> bool:
    """Compress a file using tiff-reducer."""
    cmd = [tiffthin_path, "compress", input_path, "-o", output_path]
    result = subprocess.run(cmd, capture_output=True, text=True)
    return result.returncode == 0


def main():
    if len(sys.argv) < 2:
        print("Usage: test_visual_quality.py <tiff_file> [tiffthin_binary]")
        sys.exit(1)
    
    input_file = sys.argv[1]
    tiffthin_bin = sys.argv[2] if len(sys.argv) > 2 else "./target/debug/tiff-reducer"
    
    if not os.path.exists(input_file):
        print(f"Error: File not found: {input_file}")
        sys.exit(1)
    
    if not os.path.exists(tiffthin_bin):
        print(f"Error: tiff-reducer not found: {tiffthin_bin}")
        sys.exit(1)
    
    # Create temp output file
    with tempfile.NamedTemporaryFile(suffix='.tif', delete=False) as tmp:
        output_file = tmp.name
    
    try:
        # Compress
        print(f"Compressing: {os.path.basename(input_file)}")
        if not compress_file(input_file, output_file, tiffthin_bin):
            print("Compression failed")
            sys.exit(1)
        
        # Compare
        results = compare_tiffs(input_file, output_file)
        
        if "error" in results:
            print(f"Error: {results['error']}")
            sys.exit(1)
        
        # Display results
        print(f"\n{'='*50}")
        print(f"Visual Quality Metrics")
        print(f"{'='*50}")
        print(f"Bands compared: {results['bands']}")
        print(f"Dimensions match: {results['dimensions_match']}")
        
        if results['dimensions_match']:
            print(f"\nPer-band metrics:")
            for i, (psnr, ssim) in enumerate(zip(results['psnr_per_band'], results['ssim_per_band'])):
                psnr_str = f"{psnr:.2f}" if psnr != float('inf') else "∞"
                print(f"  Band {i+1}: PSNR={psnr_str} dB, SSIM={ssim:.6f}")
            
            print(f"\nOverall:")
            psnr_str = f"{results['overall_psnr']:.2f}" if results['overall_psnr'] != float('inf') else "∞ (lossless)"
            print(f"  PSNR: {psnr_str} dB")
            print(f"  SSIM: {results['overall_ssim']:.6f}")
            
            # Quality assessment
            if results['overall_psnr'] == float('inf'):
                quality = "PERFECT (lossless compression)"
            elif results['overall_psnr'] >= 50:
                quality = "EXCELLENT"
            elif results['overall_psnr'] >= 40:
                quality = "GOOD"
            elif results['overall_psnr'] >= 30:
                quality = "ACCEPTABLE"
            else:
                quality = "POOR"
            
            print(f"\nQuality Assessment: {quality}")
        
        print(f"{'='*50}\n")
        
    finally:
        # Cleanup
        if os.path.exists(output_file):
            os.unlink(output_file)


if __name__ == "__main__":
    main()
