#!/usr/bin/env python3
"""
Generate a comprehensive test report for tiff-reducer.
Tests all images and categorizes them by working/non-working status.
Outputs a Markdown report.
"""

import subprocess
import tempfile
import os
import argparse
from pathlib import Path
from datetime import datetime

try:
    from PIL import Image
    HAS_PILLOW = True
except ImportError:
    HAS_PILLOW = False

def create_thumbnail(tiff_path, png_path, size=128):
    """Create a small thumbnail from a TIFF file."""
    if not HAS_PILLOW:
        return False
    try:
        img = Image.open(tiff_path)
        if img.mode not in ("RGB", "L"):
            if img.mode == "P":
                img = img.convert("RGB")
            elif img.mode == "I":
                img = img.point(lambda x: x >> 24 if x > 0 else 0).convert("L")
            elif img.mode == "F":
                img = img.point(lambda x: int(x * 255)).convert("L")
            else:
                img = img.convert("RGB")
        w, h = img.size
        if w > size or h > size:
            scale = size / max(w, h)
            img = img.resize((int(w * scale), int(h * scale)), Image.Resampling.LANCZOS)
        img.save(png_path)
        return True
    except Exception:
        return False

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

def test_compression(input_path, output_path, binary_path, format='zstd', level=19):
    """Test compression of a single file."""
    cmd = [
        binary_path,
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
    parser = argparse.ArgumentParser(
        description="Generate Markdown Test Report for tiff-reducer"
    )
    parser.add_argument(
        "--input", "-i", default="tests/images", help="Input directory with TIFF files"
    )
    parser.add_argument(
        "--output", "-o", default="tests/report", help="Output directory for report"
    )
    parser.add_argument(
        "--binary",
        "-b",
        default="./target/release/tiff-reducer",
        help="Path to tiff-reducer binary",
    )
    parser.add_argument("--format", "-f", default="zstd", help="Compression format")
    parser.add_argument("--level", "-l", type=int, default=19, help="Compression level")
    parser.add_argument(
        "--limit", "-n", type=int, default=None, help="Limit number of test images"
    )

    args = parser.parse_args()

    # Ensure binary exists
    binary_path = Path(args.binary)
    if not binary_path.exists():
        print(f"Error: Binary not found at {binary_path}")
        print("Run: cargo build --release")
        return 1

    # Get all test images
    test_dir = Path(args.input)
    if not test_dir.exists():
        print(f"Error: Input directory not found: {test_dir}")
        return 1

    images = sorted([f for f in test_dir.glob('*.tif*') if f.is_file()])

    # Apply limit if specified
    if args.limit:
        images = images[:args.limit]

    # Create output directories
    output_dir = Path(args.output)
    output_dir.mkdir(parents=True, exist_ok=True)
    thumbnails_dir = output_dir / 'thumbnails'
    thumbnails_dir.mkdir(parents=True, exist_ok=True)

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
            success, error = test_compression(image_path, output_path, binary_path, args.format, args.level)

            # Generate thumbnails
            thumb_orig = None
            thumb_comp = None
            if success:
                thumb_orig_name = f"{image_path.stem}_orig.png"
                thumb_comp_name = f"{image_path.stem}_comp.png"
                thumb_orig_path = thumbnails_dir / thumb_orig_name
                thumb_comp_path = thumbnails_dir / thumb_comp_name

                if create_thumbnail(image_path, thumb_orig_path):
                    thumb_orig = f"thumbnails/{thumb_orig_name}"
                if create_thumbnail(output_path, thumb_comp_path):
                    thumb_comp = f"thumbnails/{thumb_comp_name}"

            if success:
                results['working'].append({
                    'name': image_path.name,
                    'stem': image_path.stem,
                    'path': str(image_path),
                    'error': None,
                    'thumb_orig': thumb_orig,
                    'thumb_comp': thumb_comp,
                    'orig_size': image_path.stat().st_size,
                    'comp_size': output_path.stat().st_size if output_path.exists() else 0,
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
    generate_report(results, images, output_dir)
    return 0

def generate_report(results, all_images, output_dir):
    """Generate Markdown README report with thumbnails."""

    total = len(all_images)
    working = len(results['working'])
    failed = total - working

    report = []
    report.append("# tiff-reducer Test Report")
    report.append("")
    report.append(f"**Generated:** {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    report.append("")

    # Summary
    report.append("## Summary")
    report.append("")
    report.append("| Category | Count | Percentage |")
    report.append("|----------|-------|------------|")
    report.append(f"| ✅ Working | {working} | {working/total*100:.1f}% |")
    report.append(f"| ❌ Failed | {failed} | {failed/total*100:.1f}% |")
    report.append(f"| **Total** | **{total}** | **100%** |")
    report.append("")

    # Failure breakdown
    report.append("### Failure Breakdown")
    report.append("")
    report.append("| Failure Type | Count |")
    report.append("|--------------|-------|")
    report.append(f"| TIFFWriteDirectorySec crash | {len(results['failed_directory'])} |")
    report.append(f"| Read/Decode errors | {len(results['failed_read'])} |")
    report.append(f"| Tile errors | {len(results['failed_tile'])} |")
    report.append(f"| Other errors | {len(results['failed_other'])} |")
    report.append("")

    # Working images with thumbnails
    report.append("## ✅ Working Images")
    report.append("")
    report.append(f"**{working} images** successfully compressed with thumbnails below:")
    report.append("")

    # Display as a simple vertical list (more readable than table)
    working_imgs = results['working']
    for img in working_imgs:
        if img['thumb_comp']:
            reduction = (1 - img['comp_size']/img['orig_size'])*100
            report.append(f"### {img['name']}")
            report.append("")
            report.append(f"![Compressed]({img['stem']}_comp.png)")
            report.append("")
            report.append(f"- **Original size:** {img['orig_size']:,} bytes")
            report.append(f"- **Compressed size:** {img['comp_size']:,} bytes")
            report.append(f"- **Reduction:** ⬇ {reduction:.1f}%")
            report.append("")
        else:
            report.append(f"### {img['name']}")
            report.append("")
            report.append("*No thumbnail available*")
            report.append("")

    # Failed images
    all_failed = (results['failed_directory'] + results['failed_read'] +
                  results['failed_tile'] + results['failed_other'])

    report.append("## ❌ Failed Images")
    report.append("")
    if all_failed:
        report.append(f"**{len(all_failed)} images** failed to process:")
        report.append("")
        report.append("| File | Error |")
        report.append("|------|-------|")
        for img in all_failed:
            report.append(f"| `{img['name']}` | {img['error'] or 'Unknown'} |")
    else:
        report.append("*No failures!*")
    report.append("")

    # Write report
    report_path = output_dir / 'README.md'
    with open(report_path, 'w') as f:
        f.write('\n'.join(report))

    print(f"\nReport written to {report_path}")
    print(f"Thumbnails stored in {output_dir}/thumbnails/")

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
    exit(main())
