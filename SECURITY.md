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

### Security Audit Findings (v0.2.0-pre) - RESOLVED

All vulnerabilities identified during the v0.2.0 pre-release audit have been addressed:
- **RESOLVED: Out-of-Bounds Reads in `metadata.rs`**: Manual TIFF parser replaced with `libtiff`'s native API.
- **RESOLVED: Memory Safety / DoS Vulnerabilities**: Added strict bounds checking for all metadata allocations.
- **RESOLVED: FFI Buffer Safety Risks**: Added validation of bytes read from `libtiff` and checked return values for all critical FFI calls.
- **RESOLVED: Lack of BigTIFF Support**: Native `libtiff` API now handles BigTIFF offsets and entries correctly.
- **RESOLVED: Ignored FFI Return Values**: All critical FFI calls now have their return values validated.

---

### Audit History

- **2026-03-13**: Internal security audit and remediation.
  - Resolved all critical and high-risk vulnerabilities identified in the pre-release audit.
  - Refactored `metadata.rs` for native `libtiff` tag handling.
  - Hardened FFI boundaries with comprehensive error checking.
  - Verified compilation and basic functionality after security overhaul.
