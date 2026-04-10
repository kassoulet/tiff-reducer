#!/usr/bin/env python3
"""
Generate a comprehensive HTML test report for tiff-reducer.
Tests all images and categorizes them by working/non-working status.
Outputs an HTML report with thumbnails and detailed results.
"""

import subprocess
import tempfile
import os
import argparse
import html
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
            timeout=10
        )
        return result.stdout if result.returncode == 0 else ""
    except Exception:
        return ""


def get_file_size(filepath):
    """Get file size in bytes."""
    try:
        return os.path.getsize(filepath)
    except Exception:
        return 0


def format_file_size(size_bytes):
    """Format file size in human-readable format."""
    if size_bytes == 0:
        return "0 B"
    for unit in ['B', 'KB', 'MB', 'GB']:
        if size_bytes < 1024.0:
            return f"{size_bytes:.1f} {unit}"
        size_bytes /= 1024.0
    return f"{size_bytes:.1f} TB"


def test_image(binary_path, input_path, output_dir, fmt, level):
    """Test a single image with tiff-reducer."""
    with tempfile.TemporaryDirectory() as tmpdir:
        input_file = Path(input_path)
        output_file = Path(tmpdir) / f"output_{input_file.name}"

        try:
            result = subprocess.run(
                [
                    str(binary_path),
                    str(input_path),
                    str(output_file),
                    '--format', fmt,
                    '--level', str(level),
                    '--quiet'
                ],
                capture_output=True,
                text=True,
                timeout=60
            )

            if result.returncode != 0:
                return {
                    'status': 'failed',
                    'error': result.stderr.strip() or f"Exit code: {result.returncode}",
                    'output_size': 0,
                    'original_size': get_file_size(input_path),
                    'compression_ratio': 0
                }

            if not output_file.exists():
                return {
                    'status': 'failed',
                    'error': 'Output file not created',
                    'output_size': 0,
                    'original_size': get_file_size(input_path),
                    'compression_ratio': 0
                }

            output_size = get_file_size(output_file)
            original_size = get_file_size(input_path)
            ratio = (1 - output_size / original_size) * 100 if original_size > 0 else 0

            return {
                'status': 'success',
                'error': None,
                'output_size': output_size,
                'original_size': original_size,
                'compression_ratio': ratio
            }

        except subprocess.TimeoutExpired:
            return {
                'status': 'failed',
                'error': 'Timeout (60s)',
                'output_size': 0,
                'original_size': get_file_size(input_path),
                'compression_ratio': 0
            }
        except Exception as e:
            return {
                'status': 'failed',
                'error': str(e),
                'output_size': 0,
                'original_size': get_file_size(input_path),
                'compression_ratio': 0
            }


def generate_html_report(results, output_dir, fmt, level):
    """Generate an HTML report from test results."""
    output_path = Path(output_dir)
    output_path.mkdir(parents=True, exist_ok=True)
    thumbnails_dir = output_path / "thumbnails"
    thumbnails_dir.mkdir(exist_ok=True)

    total = len(results)
    passed = sum(1 for r in results if r['status'] == 'success')
    failed = total - passed
    pass_rate = (passed / total * 100) if total > 0 else 0

    avg_compression = 0
    if passed > 0:
        avg_compression = sum(r['compression_ratio'] for r in results if r['status'] == 'success') / passed

    # Categorize errors
    error_categories = {}
    for r in results:
        if r['status'] == 'failed' and r['error']:
            error_type = r['error'].split(':')[0].split('(')[0].strip()
            error_categories[error_type] = error_categories.get(error_type, 0) + 1

    now = datetime.now().strftime('%Y-%m-%d %H:%M:%S')

    html_content = f"""<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>tiff-reducer Test Report</title>
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            background: #f5f7fa;
            color: #333;
            line-height: 1.6;
            padding: 20px;
        }}
        .container {{
            max-width: 1400px;
            margin: 0 auto;
        }}
        header {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 40px;
            border-radius: 12px;
            margin-bottom: 30px;
            box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
        }}
        h1 {{
            font-size: 2.5em;
            margin-bottom: 10px;
        }}
        .subtitle {{
            font-size: 1.1em;
            opacity: 0.9;
        }}
        .summary-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 20px;
            margin-bottom: 30px;
        }}
        .summary-card {{
            background: white;
            padding: 25px;
            border-radius: 10px;
            box-shadow: 0 2px 4px rgba(0, 0, 0, 0.05);
            text-align: center;
        }}
        .summary-card .value {{
            font-size: 2.5em;
            font-weight: bold;
            margin-bottom: 5px;
        }}
        .summary-card .label {{
            color: #666;
            font-size: 0.9em;
            text-transform: uppercase;
            letter-spacing: 1px;
        }}
        .success .value {{
            color: #10b981;
        }}
        .failed .value {{
            color: #ef4444;
        }}
        .rate .value {{
            color: #3b82f6;
        }}
        .compression .value {{
            color: #8b5cf6;
        }}
        .section {{
            background: white;
            padding: 30px;
            border-radius: 10px;
            box-shadow: 0 2px 4px rgba(0, 0, 0, 0.05);
            margin-bottom: 30px;
        }}
        .section h2 {{
            font-size: 1.5em;
            margin-bottom: 20px;
            color: #1f2937;
            border-bottom: 2px solid #e5e7eb;
            padding-bottom: 10px;
        }}
        .error-list {{
            list-style: none;
        }}
        .error-list li {{
            padding: 12px 16px;
            background: #fef2f2;
            border-left: 4px solid #ef4444;
            margin-bottom: 10px;
            border-radius: 4px;
        }}
        .error-list .error-type {{
            font-weight: bold;
            color: #dc2626;
        }}
        .error-list .error-count {{
            float: right;
            background: #fee2e2;
            padding: 2px 8px;
            border-radius: 12px;
            font-size: 0.85em;
        }}
        .image-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
            gap: 20px;
        }}
        .image-card {{
            background: #f9fafb;
            border-radius: 8px;
            overflow: hidden;
            box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
            transition: transform 0.2s, box-shadow 0.2s;
        }}
        .image-card:hover {{
            transform: translateY(-2px);
            box-shadow: 0 4px 6px rgba(0, 0, 0, 0.15);
        }}
        .image-card.success {{
            border-top: 4px solid #10b981;
        }}
        .image-card.failed {{
            border-top: 4px solid #ef4444;
        }}
        .image-card img {{
            width: 100%;
            height: 150px;
            object-fit: contain;
            background: #1f2937;
        }}
        .image-card .no-thumbnail {{
            width: 100%;
            height: 150px;
            background: #e5e7eb;
            display: flex;
            align-items: center;
            justify-content: center;
            color: #6b7280;
            font-size: 0.9em;
        }}
        .image-card .card-content {{
            padding: 15px;
        }}
        .image-card .filename {{
            font-weight: 600;
            font-size: 0.95em;
            margin-bottom: 10px;
            word-break: break-all;
        }}
        .image-card .stats {{
            display: flex;
            justify-content: space-between;
            font-size: 0.85em;
            color: #6b7280;
            margin-bottom: 8px;
        }}
        .image-card .status {{
            display: inline-block;
            padding: 3px 10px;
            border-radius: 12px;
            font-size: 0.8em;
            font-weight: 600;
        }}
        .image-card .status.success {{
            background: #d1fae5;
            color: #065f46;
        }}
        .image-card .status.failed {{
            background: #fee2e2;
            color: #991b1b;
        }}
        .image-card .error-message {{
            background: #fef2f2;
            padding: 8px;
            border-radius: 4px;
            font-size: 0.8em;
            color: #dc2626;
            margin-top: 8px;
            word-break: break-word;
        }}
        footer {{
            text-align: center;
            color: #6b7280;
            padding: 20px;
            font-size: 0.9em;
        }}
    </style>
</head>
<body>
    <div class="container">
        <header>
            <h1>tiff-reducer Test Report</h1>
            <p class="subtitle">Generated: {now} | Format: {fmt} | Level: {level}</p>
        </header>

        <div class="summary-grid">
            <div class="summary-card success">
                <div class="value">{passed}</div>
                <div class="label">Passed</div>
            </div>
            <div class="summary-card failed">
                <div class="value">{failed}</div>
                <div class="label">Failed</div>
            </div>
            <div class="summary-card rate">
                <div class="value">{pass_rate:.1f}%</div>
                <div class="label">Pass Rate</div>
            </div>
            <div class="summary-card compression">
                <div class="value">{avg_compression:.1f}%</div>
                <div class="label">Avg Compression</div>
            </div>
        </div>
"""

    # Error breakdown section
    if error_categories:
        html_content += """
        <div class="section">
            <h2>Error Breakdown</h2>
            <ul class="error-list">
"""
        for error_type, count in sorted(error_categories.items(), key=lambda x: x[1], reverse=True):
            html_content += f"""                <li>
                    <span class="error-type">{html.escape(error_type)}</span>
                    <span class="error-count">{count} image(s)</span>
                </li>
"""
        html_content += """            </ul>
        </div>
"""

    # Results section
    html_content += """
        <div class="section">
            <h2>Test Results</h2>
            <div class="image-grid">
"""

    for r in results:
        thumbnail_path = r.get('thumbnail_path', '')
        filename = html.escape(r['filename'])
        original_size = format_file_size(r['original_size'])
        status_class = 'success' if r['status'] == 'success' else 'failed'
        status_text = 'Success' if r['status'] == 'success' else 'Failed'

        if r['status'] == 'success':
            output_size = format_file_size(r['output_size'])
            compression = f"{r['compression_ratio']:.1f}%"
            stats_html = f"""                <div class="stats">
                    <span>{original_size} → {output_size}</span>
                    <span>-{compression}</span>
                </div>
"""
        else:
            stats_html = f"""                <div class="stats">
                    <span>Original: {original_size}</span>
                </div>
"""

        error_html = ""
        if r['status'] == 'failed' and r['error']:
            error_html = f"""                <div class="error-message">{html.escape(r['error'])}</div>
"""

        thumbnail_html = ""
        if thumbnail_path and Path(thumbnail_path).exists():
            rel_thumbnail = os.path.relpath(thumbnail_path, output_dir)
            thumbnail_html = f'                <img src="{html.escape(rel_thumbnail)}" alt="{filename}">\n'
        else:
            thumbnail_html = '                <div class="no-thumbnail">No preview</div>\n'

        html_content += f"""                <div class="image-card {status_class}">
{thumbnail_html}                    <div class="card-content">
                        <div class="filename">{filename}</div>
{stats_html}                        <span class="status {status_class}">{status_text}</span>
{error_html}                    </div>
                </div>
"""

    html_content += """            </div>
        </div>

        <footer>
            <p>tiff-reducer Test Report | Generated automatically by CI</p>
        </footer>
    </div>
</body>
</html>
"""

    # Write HTML report
    html_file = output_path / "report.html"
    with open(html_file, 'w') as f:
        f.write(html_content)

    # Write README.md redirect
    readme_content = f"""# Test Report

View the HTML report: [report.html](report.html)

Generated: {now}
"""
    with open(output_path / "README.md", 'w') as f:
        f.write(readme_content)

    return str(html_file)


def main():
    parser = argparse.ArgumentParser(description='Generate HTML test report for tiff-reducer')
    parser.add_argument('--input', required=True, help='Input directory with test images')
    parser.add_argument('--output', required=True, help='Output directory for report')
    parser.add_argument('--binary', required=True, help='Path to tiff-reducer binary')
    parser.add_argument('--format', default='zstd', help='Compression format (default: zstd)')
    parser.add_argument('--level', type=int, default=19, help='Compression level (default: 19)')
    parser.add_argument('--limit', type=int, default=None, help='Limit number of test images')
    args = parser.parse_args()

    input_dir = Path(args.input)
    if not input_dir.exists():
        print(f"Error: Input directory not found: {input_dir}")
        return 1

    binary_path = Path(args.binary)
    if not binary_path.exists():
        print(f"Error: Binary not found: {binary_path}")
        return 1

    # Collect test images
    tiff_files = sorted(input_dir.glob('*.tif')) + sorted(input_dir.glob('*.tiff'))
    if args.limit:
        tiff_files = tiff_files[:args.limit]

    if not tiff_files:
        print("No test images found")
        return 1

    print(f"Testing {len(tiff_files)} images with {args.format} level {args.level}...")

    results = []
    for i, tiff_file in enumerate(tiff_files, 1):
        print(f"[{i}/{len(tiff_files)}] Testing {tiff_file.name}...", end=' ', flush=True)

        result = test_image(str(binary_path), str(tiff_file), args.output, args.format, args.level)
        result['filename'] = tiff_file.name

        # Create thumbnail
        thumbnail_path = Path(args.output) / "thumbnails" / f"{tiff_file.stem}.png"
        if create_thumbnail(str(tiff_file), str(thumbnail_path)):
            result['thumbnail_path'] = str(thumbnail_path)

        status_icon = '✓' if result['status'] == 'success' else '✗'
        print(f"{status_icon} {result['compression_ratio']:.1f}%" if result['status'] == 'success' else f"{status_icon} {result['error']}")

        results.append(result)

    # Generate report
    html_file = generate_html_report(results, args.output, args.format, args.level)
    print(f"\nReport generated: {html_file}")

    passed = sum(1 for r in results if r['status'] == 'success')
    failed = len(results) - passed
    print(f"Results: {passed} passed, {failed} failed")

    return 0


if __name__ == '__main__':
    exit(main())
