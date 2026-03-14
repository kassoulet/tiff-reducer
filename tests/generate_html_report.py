#!/usr/bin/env python3
"""
HTML Visual Test Report Generator for tiff-reducer

Generates an HTML report with:
- Side-by-side image comparison (thumbnails)
- Metadata comparison tables
- Pass/fail indicators with color coding
- Summary dashboard with statistics
"""

import sys
import os
import json
import subprocess
import tempfile
import shutil
from pathlib import Path
from datetime import datetime

try:
    from osgeo import gdal
    import numpy as np
except ImportError:
    print("GDAL and NumPy required. Install with: pip install gdal numpy")
    sys.exit(1)


def get_gdalinfo_json(tiff_path: str) -> dict:
    """Get GDAL info as JSON."""
    try:
        result = subprocess.run(
            ["gdalinfo", "-json", tiff_path],
            capture_output=True,
            text=True,
            timeout=30
        )
        if result.returncode == 0:
            return json.loads(result.stdout)
        return {"error": result.stderr}
    except Exception as e:
        return {"error": str(e)}


def create_thumbnail(tiff_path: str, png_path: str, size: int = 256) -> bool:
    """Convert TIFF to PNG thumbnail."""
    try:
        ds = gdal.Open(tiff_path)
        if not ds:
            return False
        
        # Read as array
        band = ds.GetRasterBand(1)
        data = band.ReadAsArray()
        
        # Handle multi-band
        if len(data.shape) == 3:
            data = data[:3, :, :]  # Take first 3 bands
            data = np.transpose(data, (1, 2, 0))  # HWC format
        
        # Normalize to 0-255
        if data.dtype != np.uint8:
            data = ((data - data.min()) / (data.max() - data.min()) * 255).astype(np.uint8)
        
        # Resize if needed
        h, w = data.shape[:2]
        if h > size or w > size:
            scale = size / max(h, w)
            new_w, new_h = int(w * scale), int(h * scale)
            from PIL import Image
            img = Image.fromarray(data)
            img = img.resize((new_w, new_h), Image.Resampling.LANCZOS)
            data = np.array(img)
        
        # Save as PNG
        from PIL import Image
        img = Image.fromarray(data)
        img.save(png_path)
        return True
    except Exception as e:
        print(f"Thumbnail error: {e}")
        return False


def create_diff_image(orig_path: str, comp_path: str, diff_path: str) -> bool:
    """Create visual difference image."""
    try:
        orig_ds = gdal.Open(orig_path)
        comp_ds = gdal.Open(comp_path)
        
        if not orig_ds or not comp_ds:
            return False
        
        band1 = orig_ds.GetRasterBand(1)
        band2 = comp_ds.GetRasterBand(1)
        
        orig_arr = band1.ReadAsArray().astype(float)
        comp_arr = band2.ReadAsArray().astype(float)
        
        # Calculate absolute difference
        diff_arr = np.abs(orig_arr - comp_arr)
        
        # Exaggerate for visibility (multiply by 10)
        diff_arr = np.clip(diff_arr * 10, 0, 255).astype(np.uint8)
        
        # Save as PNG
        from PIL import Image
        img = Image.fromarray(diff_arr, mode='L')
        img.save(diff_path)
        return True
    except Exception as e:
        print(f"Diff image error: {e}")
        return False


def compress_file(input_path: str, output_path: str, binary_path: str, format: str = "zstd", level: int = 19) -> tuple:
    """Compress a file using tiff-reducer. Returns (success, output_size, reduction)."""
    try:
        cmd = [
            binary_path, "compress",
            input_path, "-o", output_path,
            "-f", format, "-l", str(level)
        ]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
        
        if result.returncode == 0:
            orig_size = os.path.getsize(input_path)
            comp_size = os.path.getsize(output_path)
            reduction = (1 - comp_size / orig_size) * 100 if orig_size > 0 else 0
            return True, comp_size, reduction
        return False, 0, 0
    except Exception as e:
        print(f"Compression error: {e}")
        return False, 0, 0


def compare_metadata(orig_info: dict, comp_info: dict) -> list:
    """Compare metadata between original and compressed."""
    comparisons = []
    
    # Dimensions
    orig_size = orig_info.get("size", [0, 0])
    comp_size = comp_info.get("size", [0, 0])
    dim_match = orig_size == comp_size
    comparisons.append({
        "tag": "Dimensions",
        "original": f"{orig_size[0]}x{orig_size[1]}" if orig_size else "N/A",
        "compressed": f"{comp_size[0]}x{comp_size[1]}" if comp_size else "N/A",
        "status": "pass" if dim_match else "fail"
    })
    
    # Band count
    orig_bands = len(orig_info.get("bands", []))
    comp_bands = len(comp_info.get("bands", []))
    bands_match = orig_bands == comp_bands
    comparisons.append({
        "tag": "Bands",
        "original": str(orig_bands),
        "compressed": str(comp_bands),
        "status": "pass" if bands_match else "fail"
    })
    
    # Compression
    orig_comp = orig_info.get("metadata", {}).get("TIFFTAG_COMPRESSION", "None")
    comp_comp = comp_info.get("metadata", {}).get("TIFFTAG_COMPRESSION", "Unknown")
    comparisons.append({
        "tag": "Compression",
        "original": orig_comp,
        "compressed": comp_comp,
        "status": "info"
    })
    
    # Resolution
    orig_xres = orig_info.get("metadata", {}).get("TIFFTAG_XRESOLUTION", "N/A")
    comp_xres = comp_info.get("metadata", {}).get("TIFFTAG_XRESOLUTION", "N/A")
    res_match = str(orig_xres) == str(comp_xres)
    comparisons.append({
        "tag": "X Resolution",
        "original": str(orig_xres),
        "compressed": str(comp_xres),
        "status": "pass" if res_match else "warn"
    })
    
    return comparisons


def generate_html_report(test_results: list, output_dir: str):
    """Generate HTML report from test results."""
    
    os.makedirs(output_dir, exist_ok=True)
    os.makedirs(os.path.join(output_dir, "thumbnails"), exist_ok=True)
    
    # Calculate summary
    total = len(test_results)
    passed = sum(1 for r in test_results if r["success"])
    failed = total - passed
    
    html = f'''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>tiff-reducer Test Report</title>
    <style>
        * {{ box-sizing: border-box; }}
        body {{ 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0; padding: 20px; background: #f5f5f5;
        }}
        .container {{ max-width: 1400px; margin: 0 auto; }}
        h1 {{ color: #333; border-bottom: 2px solid #007bff; padding-bottom: 10px; }}
        h2 {{ color: #555; margin-top: 30px; }}
        
        .summary {{
            display: flex; gap: 20px; margin: 20px 0;
            background: white; padding: 20px; border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .summary-item {{
            text-align: center; padding: 15px 30px;
            border-radius: 8px; min-width: 150px;
        }}
        .summary-item.pass {{ background: #d4edda; color: #155724; }}
        .summary-item.fail {{ background: #f8d7da; color: #721c24; }}
        .summary-item.total {{ background: #e2e3e5; color: #383d41; }}
        .summary-value {{ font-size: 2em; font-weight: bold; }}
        .summary-label {{ font-size: 0.9em; opacity: 0.8; }}
        
        .test-case {{
            background: white; margin: 20px 0; padding: 20px;
            border-radius: 8px; border-left: 5px solid #ccc;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .test-case.pass {{ border-left-color: #28a745; }}
        .test-case.fail {{ border-left-color: #dc3545; }}
        
        .test-header {{
            display: flex; justify-content: space-between; align-items: center;
            margin-bottom: 15px;
        }}
        .test-name {{ font-size: 1.2em; font-weight: bold; color: #333; }}
        .test-status {{
            padding: 5px 15px; border-radius: 20px; font-weight: bold;
        }}
        .test-status.pass {{ background: #28a745; color: white; }}
        .test-status.fail {{ background: #dc3545; color: white; }}
        
        .image-comparison {{
            display: flex; gap: 15px; flex-wrap: wrap; margin: 15px 0;
        }}
        .image-panel {{
            text-align: center; background: #f8f9fa;
            padding: 10px; border-radius: 5px;
        }}
        .image-panel img {{
            max-width: 256px; max-height: 256px;
            border: 1px solid #ddd; border-radius: 4px;
        }}
        .image-panel h4 {{ margin: 10px 0 5px; font-size: 0.9em; color: #555; }}
        .image-panel .size {{ font-size: 0.8em; color: #888; }}
        
        .metadata-table {{
            width: 100%; border-collapse: collapse; margin: 15px 0;
            font-size: 0.9em;
        }}
        .metadata-table th, .metadata-table td {{
            border: 1px solid #ddd; padding: 8px; text-align: left;
        }}
        .metadata-table th {{ background: #f8f9fa; font-weight: 600; }}
        .metadata-table tr:nth-child(even) {{ background: #f8f9fa; }}
        .status-pass {{ color: #28a745; font-weight: bold; }}
        .status-fail {{ color: #dc3545; font-weight: bold; }}
        .status-warn {{ color: #ffc107; font-weight: bold; }}
        .status-info {{ color: #17a2b8; font-weight: bold; }}
        
        .metrics {{
            display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
            gap: 10px; margin: 15px 0;
        }}
        .metric {{
            background: #f8f9fa; padding: 10px; border-radius: 5px;
            text-align: center;
        }}
        .metric-value {{ font-size: 1.3em; font-weight: bold; color: #333; }}
        .metric-label {{ font-size: 0.8em; color: #666; }}
        
        .error {{ background: #f8d7da; color: #721c24; padding: 10px; border-radius: 5px; }}
        
        footer {{
            margin-top: 40px; padding-top: 20px;
            border-top: 1px solid #ddd; color: #666; font-size: 0.9em;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>📊 tiff-reducer Test Report</h1>
        <p>Generated: {datetime.now().strftime("%Y-%m-%d %H:%M:%S")}</p>
        
        <div class="summary">
            <div class="summary-item pass">
                <div class="summary-value">{passed}</div>
                <div class="summary-label">Passed</div>
            </div>
            <div class="summary-item fail">
                <div class="summary-value">{failed}</div>
                <div class="summary-label">Failed</div>
            </div>
            <div class="summary-item total">
                <div class="summary-value">{total}</div>
                <div class="summary-label">Total</div>
            </div>
        </div>
        
        <h2>Test Cases</h2>
'''
    
    for result in test_results:
        status_class = "pass" if result["success"] else "fail"
        status_text = "✅ PASS" if result["success"] else "❌ FAIL"
        
        html += f'''
        <div class="test-case {status_class}">
            <div class="test-header">
                <span class="test-name">{result["name"]}</span>
                <span class="test-status {status_class}">{status_text}</span>
            </div>
'''
        
        if result["success"]:
            # Image comparison
            html += f'''
            <div class="image-comparison">
                <div class="image-panel">
                    <h4>Original</h4>
                    <img src="thumbnails/{result["thumb_orig"]}" alt="Original">
                    <div class="size">{result["orig_size"]:,} bytes</div>
                </div>
                <div class="image-panel">
                    <h4>Compressed</h4>
                    <img src="thumbnails/{result["thumb_comp"]}" alt="Compressed">
                    <div class="size">{result["comp_size"]:,} bytes ({result["reduction"]:.1f}% reduction)</div>
                </div>
                <div class="image-panel">
                    <h4>Difference</h4>
                    <img src="thumbnails/{result["thumb_diff"]}" alt="Difference">
                    <div class="size">Exaggerated 10x</div>
                </div>
            </div>
'''
            
            # Metrics
            if "psnr" in result:
                psnr_str = "∞" if result["psnr"] == float('inf') else f'{result["psnr"]:.2f}'
                html += f'''
            <div class="metrics">
                <div class="metric">
                    <div class="metric-value">{psnr_str}</div>
                    <div class="metric-label">PSNR (dB)</div>
                </div>
                <div class="metric">
                    <div class="metric-value">{result.get("ssim", 0):.6f}</div>
                    <div class="metric-label">SSIM</div>
                </div>
                <div class="metric">
                    <div class="metric-value">{result["reduction"]:.1f}%</div>
                    <div class="metric-label">Reduction</div>
                </div>
            </div>
'''
            
            # Metadata table
            if "comparisons" in result:
                html += '''
            <table class="metadata-table">
                <tr><th>Tag</th><th>Original</th><th>Compressed</th><th>Status</th></tr>
'''
                for comp in result["comparisons"]:
                    status_class = f'status-{comp["status"]}'
                    status_symbol = {"pass": "✅", "fail": "❌", "warn": "⚠️", "info": "ℹ️"}.get(comp["status"], "")
                    html += f'''
                <tr>
                    <td>{comp["tag"]}</td>
                    <td>{comp["original"]}</td>
                    <td>{comp["compressed"]}</td>
                    <td class="{status_class}">{status_symbol}</td>
                </tr>
'''
                html += '''
            </table>
'''
        else:
            # Error message
            html += f'''
            <div class="error">
                <strong>Error:</strong> {result.get("error", "Unknown error")}
            </div>
'''
        
        html += '''
        </div>
'''
    
    html += '''
        <footer>
            <p>Generated by tiff-reducer HTML Test Report Generator</p>
        </footer>
    </div>
</body>
</html>
'''
    
    # Write HTML
    html_path = os.path.join(output_dir, "index.html")
    with open(html_path, 'w') as f:
        f.write(html)
    
    print(f"Report generated: {html_path}")
    return html_path


def run_tests(test_images: list, binary_path: str, output_dir: str, format: str = "zstd", level: int = 19):
    """Run tests on a list of images and generate report."""
    
    test_results = []
    
    for image_path in test_images:
        name = os.path.basename(image_path)
        print(f"Testing: {name}")
        
        with tempfile.TemporaryDirectory() as tmpdir:
            comp_path = os.path.join(tmpdir, "compressed.tif")
            
            # Compress
            success, comp_size, reduction = compress_file(image_path, comp_path, binary_path, format, level)
            orig_size = os.path.getsize(image_path)
            
            if not success:
                test_results.append({
                    "name": name,
                    "success": False,
                    "error": "Compression failed",
                    "orig_size": orig_size,
                    "comp_size": 0,
                    "reduction": 0
                })
                continue
            
            # Get metadata
            orig_info = get_gdalinfo_json(image_path)
            comp_info = get_gdalinfo_json(comp_path)
            
            if "error" in orig_info or "error" in comp_info:
                test_results.append({
                    "name": name,
                    "success": False,
                    "error": "GDAL metadata extraction failed",
                    "orig_size": orig_size,
                    "comp_size": comp_size,
                    "reduction": reduction
                })
                continue
            
            # Compare metadata
            comparisons = compare_metadata(orig_info, comp_info)
            
            # Calculate quality metrics
            try:
                orig_ds = gdal.Open(image_path)
                comp_ds = gdal.Open(comp_path)
                
                psnr_values = []
                ssim_values = []
                
                for b in range(1, min(orig_ds.RasterCount, comp_ds.RasterCount) + 1):
                    orig_arr = orig_ds.GetRasterBand(b).ReadAsArray()
                    comp_arr = comp_ds.GetRasterBand(b).ReadAsArray()
                    
                    mse = np.mean((orig_arr.astype(float) - comp_arr.astype(float)) ** 2)
                    if mse > 0:
                        max_val = np.iinfo(orig_arr.dtype).max if np.issubdtype(orig_arr.dtype, np.integer) else 1.0
                        psnr = 10 * np.log10((max_val ** 2) / mse)
                        psnr_values.append(psnr)
                    
                    # Simple SSIM approximation
                    ssim = 1.0 / (1.0 + np.std(orig_arr.astype(float) - comp_arr.astype(float)) / 255.0)
                    ssim_values.append(ssim)
                
                psnr = float('inf') if not psnr_values else np.mean(psnr_values)
                ssim = np.mean(ssim_values) if ssim_values else 0
                
            except Exception as e:
                psnr = 0
                ssim = 0
            
            # Generate thumbnails
            thumb_orig = f"{name}_orig.png"
            thumb_comp = f"{name}_comp.png"
            thumb_diff = f"{name}_diff.png"
            
            create_thumbnail(image_path, os.path.join(output_dir, "thumbnails", thumb_orig))
            create_thumbnail(comp_path, os.path.join(output_dir, "thumbnails", thumb_comp))
            create_diff_image(image_path, comp_path, os.path.join(output_dir, "thumbnails", thumb_diff))
            
            # Check if all metadata comparisons passed
            all_pass = all(c["status"] in ["pass", "info"] for c in comparisons)
            
            test_results.append({
                "name": name,
                "success": all_pass,
                "orig_size": orig_size,
                "comp_size": comp_size,
                "reduction": reduction,
                "psnr": psnr,
                "ssim": ssim,
                "comparisons": comparisons,
                "thumb_orig": thumb_orig,
                "thumb_comp": thumb_comp,
                "thumb_diff": thumb_diff
            })
    
    # Generate HTML report
    generate_html_report(test_results, output_dir)
    
    # Print summary
    passed = sum(1 for r in test_results if r["success"])
    print(f"\n{'='*50}")
    print(f"Test Summary: {passed}/{len(test_results)} passed")
    print(f"Report: {output_dir}/index.html")
    print(f"{'='*50}")
    
    return test_results


def main():
    import argparse
    
    parser = argparse.ArgumentParser(description="Generate HTML Visual Test Report for tiff-reducer")
    parser.add_argument("--input", "-i", default="tests/images", help="Input directory with TIFF files")
    parser.add_argument("--output", "-o", default="tests/report", help="Output directory for report")
    parser.add_argument("--binary", "-b", default="./target/release/tiff-reducer", help="Path to tiff-reducer binary")
    parser.add_argument("--format", "-f", default="zstd", help="Compression format")
    parser.add_argument("--level", "-l", type=int, default=19, help="Compression level")
    parser.add_argument("--limit", "-n", type=int, default=20, help="Limit number of test images")
    
    args = parser.parse_args()
    
    # Find test images
    input_dir = Path(args.input)
    if not input_dir.exists():
        print(f"Error: Input directory not found: {input_dir}")
        sys.exit(1)
    
    test_images = list(input_dir.glob("*.tif")) + list(input_dir.glob("*.tiff"))
    test_images = sorted(test_images)[:args.limit]
    
    if not test_images:
        print(f"No TIFF files found in {input_dir}")
        sys.exit(1)
    
    # Check binary
    if not os.path.exists(args.binary):
        print(f"Error: Binary not found: {args.binary}")
        print("Run: cargo build --release")
        sys.exit(1)
    
    # Run tests
    run_tests(
        [str(p) for p in test_images],
        args.binary,
        args.output,
        args.format,
        args.level
    )


if __name__ == "__main__":
    main()
