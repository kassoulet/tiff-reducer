# Security Policy

## Security Considerations

This crate uses FFI to interface with libtiff, which requires careful handling of unsafe code.

### Unsafe Code Review

All `unsafe` blocks in this codebase are used for:
1. **FFI calls to libtiff** - Required for TIFF file operations
2. **Raw pointer operations** - Required for passing data to/from libtiff

### Security Features Implemented

#### Input Validation
- **NaN/Infinity handling**: The quantization module properly handles NaN and infinite float values
- **Buffer bounds checking**: All buffer operations check bounds before access
- **Empty input handling**: Functions gracefully handle empty inputs

#### Error Handling
- **CString creation**: Uses `Result` instead of `unwrap()` to handle invalid paths
- **Null pointer checks**: All FFI returned pointers are checked for null
- **Graceful degradation**: Invalid inputs produce errors, not panics

#### Memory Safety
- **No raw pointer arithmetic**: All pointer operations use safe Rust abstractions
- **Proper resource cleanup**: TIFF handles are always closed via `TIFFClose()`
- **No use-after-free**: Data is copied before FFI calls when needed

### Known Limitations

1. **libtiff vulnerabilities**: This crate depends on libtiff. Keep libtiff updated when using system libraries.
2. **Malformed TIFF files**: While we handle errors gracefully, libtiff itself may have vulnerabilities. Consider fuzz testing for your use case.
3. **File path validation**: Paths with null bytes will fail gracefully, but consider validating paths at application boundaries.

### Reporting Security Vulnerabilities

If you discover a security vulnerability, please report it privately to:
- **Email**: kassoulet@gmail.com
- **GitHub Security Advisories**: Use the "Security" tab

Please include:
1. Description of the vulnerability
2. Steps to reproduce
3. Potential impact
4. Suggested fix (if any)

### Security Best Practices for Users

1. **Validate input files**: Don't process untrusted TIFF files without validation
2. **Use vendored builds**: For production, use `--features vendored` to control libtiff version
3. **Keep dependencies updated**: Regularly update libtiff and this crate
4. **Run in sandbox**: Consider running in a container or sandbox for untrusted inputs
5. **Enable fuzzing**: Use the provided fuzz tests to validate error handling

---

## Security Audit - March 2026

A comprehensive security audit was performed on 2026-03-22, identifying 18 security issues across the codebase.

### Audit Summary

| Severity | Count | Status |
|----------|-------|--------|
| Critical | 2 | ✅ Fixed |
| High | 8 | ✅ Fixed |
| Medium | 6 | ✅ Fixed |
| Low | 2 | ✅ Fixed |

### Critical Findings

#### 1. Unchecked FFI Return Value in TIFFSetField (metadata.rs:47-50)

**Issue:** The return value of `TIFFSetField` is ignored in `copy_colormap()`. If the set operation fails, the destination TIFF may be left in an inconsistent state.

**Status:** ✅ **Fixed**

**Fix Applied:**
```rust
if TIFFSetField(dst, TIFFTAG_COLORMAP, rmap, gmap, bmap) == 0 {
    return Err(anyhow!("Failed to set colormap"));
}
```

#### 2. Path Traversal Vulnerability (main.rs:268-278)

**Issue:** User-controlled file names are directly joined with output paths without sanitization. An attacker could craft a file name like `../../../etc/passwd` to write outside the intended directory.

**Status:** ✅ **Fixed**

**Fix Applied:**
```rust
fn sanitize_filename(name: &std::ffi::OsStr) -> Option<String> {
    let path = Path::new(name);
    for component in path.components() {
        if let Component::ParentDir = component {
            return None;  // Reject paths with ".."
        }
    }
    name.to_str().map(|s| s.to_string())
}
```

### High Severity Findings

| # | Issue | File | Status |
|---|-------|------|--------|
| 3 | Buffer overflow via unvalidated scanline size | main.rs:693-701 | ✅ Fixed |
| 4 | Null pointer dereference in analyze_file | main.rs:183-203 | ✅ Fixed |
| 5 | Use-after-free risk in metadata copying | metadata.rs:56-65 | ✅ Fixed |
| 6 | Integer overflow in tiled image processing | main.rs:800-805 | ✅ Fixed |
| 7 | Missing bounds check in tile processing | main.rs:827-832 | ✅ Fixed |
| 8 | Unvalidated compression level input | main.rs:637-646 | ✅ Fixed |

### Medium Severity Findings

| # | Issue | File | Status |
|---|-------|------|--------|
| 9 | Information leakage in error messages | main.rs:253-258 | ✅ Fixed |
| 10 | Panic on unwrap in file processing | main.rs:265 | ✅ Fixed |
| 11 | Missing validation in get_sample_format | main.rs:508-517 | ✅ Fixed |
| 12 | Missing unsafe documentation | main.rs:569 | ✅ Fixed |
| 13 | DoS via temp file exhaustion (extreme mode) | main.rs:389-420 | ✅ Fixed |
| 14 | Unchecked TIFFReadDirectory return value | main.rs:556-559 | ✅ Fixed |

### Low Severity Findings

| # | Issue | File | Status |
|---|-------|------|--------|
| 15 | Hardcoded path in integration tests | integration_tests.rs:89 | ✅ Fixed |
| 16 | Missing input validation for empty files | main.rs:536-540 | ✅ Fixed |
| 17 | Inconsistent safety annotation | quantize.rs:5-7 | ✅ Fixed |
| 18 | ICC profile size check weakness | metadata.rs:77-82 | ✅ Fixed |

---

## Remediation Status

All 18 security issues identified in the March 2026 audit have been addressed:

### Phase 1: Critical Fixes ✅ COMPLETED
- ✅ Path traversal vulnerability fixed with filename sanitization
- ✅ Return value checking added for all TIFFSetField calls

### Phase 2: High Severity ✅ COMPLETED
- ✅ Bounds checking added for FFI-returned sizes (MAX_SCANLINE_SIZE limit)
- ✅ Integer overflow protection implemented with `checked_*` methods in tiled processing
- ✅ FFI pointers copied to local buffers before cross-handle use
- ✅ Null pointer validation added in analyze_file

### Phase 3: Medium Severity ✅ COMPLETED
- ✅ Structured error handling with sanitized messages
- ✅ All `unwrap()` calls replaced with proper error handling
- ✅ Safety documentation added to all `unsafe` functions
- ✅ Temporary directories with automatic cleanup
- ✅ EOF distinguished from error conditions in directory reading

### Phase 4: Low Severity ✅ COMPLETED
- ✅ Hardcoded paths removed from tests
- ✅ Minimum file size validation added
- ✅ Safety annotations reviewed and fixed
- ✅ ICC profile size validation strengthened (100MB limit)

---

## Previous Audit History

### Audit - 2026-03-13: Internal Security Audit

All vulnerabilities identified during the v0.2.0 pre-release audit were addressed:
- **RESOLVED**: Out-of-Bounds reads in `metadata.rs` - Manual TIFF parser replaced with libtiff's native API
- **RESOLVED**: Memory Safety / DoS vulnerabilities - Added strict bounds checking for all metadata allocations
- **RESOLVED**: FFI Buffer Safety Risks - Added validation of bytes read from libtiff
- **RESOLVED**: Lack of BigTIFF Support - Native libtiff API now handles BigTIFF correctly
- **RESOLVED**: Ignored FFI Return Values - All critical FFI calls now have return values validated

---

## Security Testing Recommendations

### For Developers
1. Run `cargo audit` regularly to check for dependency vulnerabilities
2. Use `cargo fuzz` with libFuzzer to test edge cases
3. Enable all clippy lints: `cargo clippy -- -D warnings`
4. Run tests with MIRI for undefined behavior detection: `cargo +nightly miri test`

### For Users
1. Process files in a sandboxed environment (container, VM, or sandbox)
2. Implement file size limits appropriate for your use case
3. Validate output files before further processing
4. Monitor for unusual resource consumption during processing

---

*Last updated: 2026-03-24*
*Next scheduled audit: 2026-06-24*
