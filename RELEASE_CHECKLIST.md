# Release Checklist for v0.3.1

This document tracks the release readiness of tiff-reducer v0.3.1.

## Version Information

- **Version**: 0.3.1
- **Release Date**: 2026-03-22
- **Previous Version**: 0.2.0 (2026-03-13)

---

## Pre-Release Checklist

### Code Quality
- [x] All tests passing (6/6 integration tests)
- [x] No compiler warnings
- [x] Clippy checks pass
- [x] Code formatted with `cargo fmt`
- [x] No `unwrap()` on external input (uses `Result` handling)

### Documentation
- [x] README.md updated with current features
- [x] CHANGELOG.md follows Keep a Changelog format
- [x] ROADMAP.md aligned with current status
- [x] SECURITY.md includes audit findings
- [x] Test reports generated (tests/README.md)
- [x] Thumbnail images for GitHub display

### Version Consistency
- [x] Cargo.toml: version = "0.3.1"
- [x] CHANGELOG.md: [0.3.1] section complete
- [x] ROADMAP.md: v0.3.1 marked as completed
- [x] README.md: References v0.3.1

### Testing
- [x] Integration tests: 6/6 passing
- [x] Image compression: 292/304 (96.1% success)
- [x] Metadata preservation: All key tags preserved
- [x] GeoTIFF test: PASSED
- [x] Error handling: Graceful failures

### Security
- [x] Security audit completed (March 2026)
- [x] 18 issues documented in SECURITY.md
- [x] Remediation plan defined (4 phases)
- [x] Known limitations documented

### Build Verification
- [x] Release build compiles: `cargo build --release`
- [ ] Vendored build works: `cargo build --release --features vendored`
- [ ] Docker build works (if applicable)
- [ ] Binary runs: `tiff-reducer --help`

### Git Repository
- [x] All changes committed
- [x] No untracked files (except build artifacts)
- [x] Git history clean (no co-author trailers)
- [x] Branch: develop (ready to merge/tag)

---

## Release Artifacts

### Expected Outputs
- [ ] GitHub release: v0.3.1
- [ ] Git tag: v0.3.1
- [ ] crates.io publish: `cargo publish`
- [ ] Release notes published

### Binary Distribution
- [ ] Linux x86_64 binary
- [ ] Docker image (optional)
- [ ] Installation instructions verified

---

## Known Issues (Documented)

### Not Blocking Release
1. **YCbCr subsampling crash** - libtiff upstream bug, documented
2. **OJPEG/THUNDERSCAN** - Legacy formats, documented as unsupported
3. **Security audit findings** - 18 issues identified, remediation planned

### To Be Fixed in v0.4.0
- Path traversal vulnerability (Critical)
- Unchecked FFI return values (Critical)
- 8 High severity security issues
- 6 Medium severity issues
- 2 Low severity issues

---

## Release Commands

### Create Git Tag
```bash
git tag -a v0.3.1 -m "Release v0.3.1 - HTML Visual Test Reports, GeoTIFF Support, LibTIFF v4.7.1"
git push origin v0.3.1
```

### Publish to crates.io
```bash
# Verify clean state
cargo clean
cargo build --release

# Test locally
cargo test --release

# Publish (requires crates.io account)
cargo publish
```

### Create GitHub Release
1. Go to https://github.com/kassoulet/tiff-reducer/releases
2. Click "Create a new release"
3. Tag version: v0.3.1
4. Target: develop branch
5. Release title: v0.3.1 - HTML Visual Test Reports, GeoTIFF Support
6. Copy changelog from CHANGELOG.md
7. Attach binaries (optional)

---

## Post-Release Checklist

### After Publishing
- [ ] Verify crates.io page: https://crates.io/crates/tiff-reducer
- [ ] Verify GitHub release: https://github.com/kassoulet/tiff-reducer/releases
- [ ] Update website/documentation links
- [ ] Announce release (if applicable)

### Follow-up Tasks
- [ ] Begin Phase 1 security remediation
- [ ] Update ROADMAP.md with v0.4.0 timeline
- [ ] Monitor for user-reported issues

---

## Release Notes Summary

### What's New in v0.3.1

**Major Features:**
- HTML Visual Test Reports with side-by-side image comparisons
- GeoTIFF compression and metadata preservation (libgeotiff integration)
- LibTIFF v4.7.1 upgrade (vendored)
- Improved tiled TIFF processing

**Test Coverage:**
- 6/6 integration tests passing (100%)
- 292/304 images compress successfully (96.1%)
- Full GeoTIFF metadata preservation (coordinate system, origin, pixel size)
- 559 thumbnail images for visual comparison

**Documentation:**
- Comprehensive security audit (18 issues documented)
- Updated README with usage examples
- Test reports with GitHub-friendly formatting (tests/README.md)
- Security policy updated (SECURITY.md)

**Known Limitations:**
- YCbCr with subsampling causes libtiff crash (upstream bug)
- Legacy formats (OJPEG, THUNDERSCAN) not supported
- Security remediation planned for v0.4.0

---

*Checklist created: 2026-03-22*
*Last updated: 2026-03-22 (v0.3.1)*
*Release manager: Gautier Portet*
