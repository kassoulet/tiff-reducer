## 2026-03-22 - [Critical] Unchecked FFI Return Values in libtiff
**Vulnerability:** Ignoring return values from TIFFSetField and TIFFWriteDirectory.
**Learning:** In C-based FFI, silent failures can lead to inconsistent state or malformed files. libtiff functions return 0 on failure, which must be explicitly checked.
**Prevention:** Always wrap FFI calls that modify state in error-checking logic and propagate errors to the caller.
