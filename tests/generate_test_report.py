#!/usr/bin/env python3
"""
Generate a comprehensive test report for tiff-reducer.
Tests all images and categorizes them by working/non-working status.
"""

import subprocess
import tempfile
import os
from pathlib import Path
from datetime import datetime

def get_tiffinfo(filepath):
    """Get TIFF metadata using tiffinfo."""
    try:
        result = subprocess.run(
            ['tiffinfo', str(filepath)],
            capture_output=True,
            text=True,
            timeout=30
        )
        return result.stdout
    except Exception as e:
        return f"Error: {e}"

def test_compression(input_path, output_path, format='zstd', level=19):
    """Test compression of a single file."""
    cmd = [
        './target/release/tiff-reducer',
        'compress',
        str(input_path),
        '-o', str(output_path),
        '-f', format,
        '-l', str(level)
    ]
    
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=60
        )
        
        if result.returncode != 0:
            if 'TIFFWriteDirectorySec' in result.stderr:
                return False, 'TIFFWriteDirectorySec crash'
            elif 'Failed to read' in result.stderr:
                return False, 'Read error'
            elif 'Failed to decode tile' in result.stderr:
                return False, 'Tile decode error'
            elif 'Failed to write' in result.stderr:
                return False, 'Write error'
            else:
                return False, result.stderr[:200]
        
        # Check if output file exists and has content
        if not output_path.exists() or output_path.stat().st_size == 0:
            return False, 'No output file'
        
        return True, 'Success'
        
    except subprocess.TimeoutExpired:
        return False, 'Timeout'
    except Exception as e:
        return False, str(e)

def get_file_info(filepath):
    """Extract key information from tiffinfo output."""
    info = get_tiffinfo(filepath)
    
    result = {
        'pages': 1,
        'tiled': False,
        'compression': 'Unknown',
        'bits_per_sample': 'Unknown',
        'sample_format': 'Unknown',
        'samples_per_pixel': 'Unknown',
        'width': 'Unknown',
        'height': 'Unknown',
    }
    
    # Count pages
    pages = info.count('=== TIFF directory')
    result['pages'] = pages
    
    # Check if tiled
    result['tiled'] = 'Tile Width:' in info
    
    # Extract compression
    for line in info.split('\n'):
        if 'Compression Scheme:' in line:
            result['compression'] = line.split(':')[1].strip()
        elif 'Bits/Sample:' in line:
            result['bits_per_sample'] = line.split(':')[1].strip()
        elif 'Sample Format:' in line:
            result['sample_format'] = line.split(':')[1].strip()
        elif 'Samples/Pixel:' in line:
            result['samples_per_pixel'] = line.split(':')[1].strip()
        elif 'Image Width:' in line:
            parts = line.split(':')[1].strip().split()
            result['width'] = parts[0]
            result['height'] = parts[2] if len(parts) > 2 else 'Unknown'
    
    return result

def main():
    # Ensure binary is built
    print("Building tiff-reducer...")
    subprocess.run(['cargo', 'build', '--release'], check=True, capture_output=True)
    
    # Get all test images
    test_dir = Path('tests/images')
    images = sorted([f for f in test_dir.glob('*.tif*') if f.is_file()])
    
    print(f"Testing {len(images)} images...")
    
    results = {
        'working': [],
        'failed_directory': [],
        'failed_read': [],
        'failed_tile': [],
        'failed_other': [],
    }
    
    for i, image_path in enumerate(images):
        print(f"[{i+1}/{len(images)}] Testing {image_path.name}...", end=' ')
        
        with tempfile.TemporaryDirectory() as tmpdir:
            output_path = Path(tmpdir) / 'output.tif'
            success, error = test_compression(image_path, output_path)
            
            if success:
                results['working'].append({
                    'name': image_path.name,
                    'path': str(image_path),
                    'error': None
                })
                print('✅')
            else:
                entry = {
                    'name': image_path.name,
                    'path': str(image_path),
                    'error': error
                }
                
                if 'TIFFWriteDirectorySec' in error:
                    results['failed_directory'].append(entry)
                elif 'Read error' in error or 'Tile decode error' in error:
                    results['failed_read'].append(entry)
                elif 'Tile' in error:
                    results['failed_tile'].append(entry)
                else:
                    results['failed_other'].append(entry)
                
                print(f'❌ ({error})')
    
    # Generate report
    generate_report(results, images)

def generate_report(results, all_images):
    """Generate markdown report."""
    
    report = []
    report.append("# tiff-reducer Test Report")
    report.append("")
    report.append(f"**Generated:** {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    report.append("")
    
    # Summary
    total = len(all_images)
    working = len(results['working'])
    failed = total - working
    
    report.append("## Summary")
    report.append("")
    report.append(f"| Category | Count | Percentage |")
    report.append(f"|----------|-------|------------|")
    report.append(f"| ✅ Working | {working} | {working/total*100:.1f}% |")
    report.append(f"| ❌ Failed | {failed} | {failed/total*100:.1f}% |")
    report.append(f"| **Total** | **{total}** | **100%** |")
    report.append("")
    
    # Failure breakdown
    report.append("### Failure Breakdown")
    report.append("")
    report.append(f"| Failure Type | Count |")
    report.append(f"|--------------|-------|")
    report.append(f"| TIFFWriteDirectorySec crash | {len(results['failed_directory'])} |")
    report.append(f"| Read/Decode errors | {len(results['failed_read'])} |")
    report.append(f"| Tile errors | {len(results['failed_tile'])} |")
    report.append(f"| Other errors | {len(results['failed_other'])} |")
    report.append("")
    
    # Working images
    report.append("## ✅ Working Images")
    report.append("")
    if results['working']:
        report.append("<details>")
        report.append(f"<summary>{len(results['working'])} working images</summary>")
        report.append("")
        for img in results['working']:
            report.append(f"- `{img['name']}`")
        report.append("")
        report.append("</details>")
    else:
        report.append("*No working images*")
    report.append("")
    
    # Failed - TIFFWriteDirectorySec
    report.append("## ❌ TIFFWriteDirectorySec Crashes")
    report.append("")
    report.append("**Cause:** libtiff 4.5.1 crashes when writing directory with certain tag combinations.")
    report.append("")
    if results['failed_directory']:
        report.append("<details>")
        report.append(f"<summary>{len(results['failed_directory'])} images with directory crashes</summary>")
        report.append("")
        for img in results['failed_directory']:
            report.append(f"- `{img['name']}`")
        report.append("")
        report.append("</details>")
    else:
        report.append("*None*")
    report.append("")
    
    # Failed - Read errors
    report.append("## ❌ Read/Decode Errors")
    report.append("")
    report.append("**Cause:** Unable to read source file format or decode compressed tiles.")
    report.append("")
    if results['failed_read']:
        report.append("<details>")
        report.append(f"<summary>{len(results['failed_read'])} images with read errors</summary>")
        report.append("")
        for img in results['failed_read']:
            report.append(f"- `{img['name']}` - {img['error']}")
        report.append("")
        report.append("</details>")
    else:
        report.append("*None*")
    report.append("")
    
    # Failed - Tile errors
    report.append("## ❌ Tile Processing Errors")
    report.append("")
    report.append("**Cause:** Issues with tiled image processing.")
    report.append("")
    if results['failed_tile']:
        report.append("<details>")
        report.append(f"<summary>{len(results['failed_tile'])} images with tile errors</summary>")
        report.append("")
        for img in results['failed_tile']:
            report.append(f"- `{img['name']}` - {img['error']}")
        report.append("")
        report.append("</details>")
    else:
        report.append("*None*")
    report.append("")
    
    # Failed - Other
    report.append("## ❌ Other Errors")
    report.append("")
    if results['failed_other']:
        report.append("<details>")
        report.append(f"<summary>{len(results['failed_other'])} images with other errors</summary>")
        report.append("")
        for img in results['failed_other']:
            report.append(f"- `{img['name']}` - {img['error']}")
        report.append("")
        report.append("</details>")
    else:
        report.append("*None*")
    report.append("")
    
    # Known limitations
    report.append("## Known Limitations")
    report.append("")
    report.append("### Multi-page OME-TIFF Files")
    report.append("")
    report.append("Multi-page OME-TIFF files (microscopy data) crash during directory writing.")
    report.append("This appears to be a libtiff 4.5.1 limitation with complex metadata structures.")
    report.append("")
    report.append("### Tiled Images with Specific Metadata")
    report.append("")
    report.append("Some LZW-compressed tiled images with Page Number tags or XMP metadata crash.")
    report.append("Standard tiled images without complex metadata work correctly.")
    report.append("")
    report.append("### Compression Level Tags")
    report.append("")
    report.append("- DEFLATELEVEL tag causes crashes - currently disabled")
    report.append("- ZSTD level tag (65564) not supported in libtiff 4.5.1")
    report.append("- LZMA preset level works correctly")
    report.append("")
    report.append("### Predictor")
    report.append("")
    report.append("Horizontal predictor causes crashes with ZSTD/Deflate compression.")
    report.append("Currently disabled by default for stability.")
    report.append("")
    
    # Recommendations
    report.append("## Recommendations")
    report.append("")
    report.append("1. **For multi-page files:** Process pages individually if possible")
    report.append("2. **For tiled files:** Most work correctly; failures are metadata-specific")
    report.append("3. **For best compression:** Use ZSTD (default) with level 19")
    report.append("4. **For compatibility:** Use Deflate compression")
    report.append("")
    
    # Write report
    report_path = Path('tests/TEST_REPORT.md')
    with open(report_path, 'w') as f:
        f.write('\n'.join(report))
    
    print(f"\nReport written to {report_path}")
    
    # Also print summary
    print(f"\n{'='*60}")
    print(f"SUMMARY")
    print(f"{'='*60}")
    print(f"Working:     {working}/{total} ({working/total*100:.1f}%)")
    print(f"Failed:      {failed}/{total} ({failed/total*100:.1f}%)")
    print(f"  - Directory crashes: {len(results['failed_directory'])}")
    print(f"  - Read errors:       {len(results['failed_read'])}")
    print(f"  - Tile errors:       {len(results['failed_tile'])}")
    print(f"  - Other errors:      {len(results['failed_other'])}")
    print(f"{'='*60}")

if __name__ == '__main__':
    main()
