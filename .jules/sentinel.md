## 2026-03-22 - [Critical] Unchecked FFI Return Values in libtiff
**Vulnerability:** Ignoring return values from TIFFSetField and TIFFWriteDirectory.
**Learning:** In C-based FFI, silent failures can lead to inconsistent state or malformed files. libtiff functions return 0 on failure, which must be explicitly checked.
**Prevention:** Always wrap FFI calls that modify state in error-checking logic and propagate errors to the caller.

## 2026-03-22 - [Critical] Integer Overflow and Division by Zero in FFI-returned values
**Vulnerability:** Calculating buffer sizes and performing divisions using raw, unvalidated values from TIFF headers (e.g., width, samples per pixel, tile dimensions).
**Learning:** Malformed TIFF files can contain zero or extremely large values for dimensions and sample counts. Direct multiplication can overflow, and division by zero leads to immediate panics.
**Prevention:** Use `checked_mul` for size calculations and explicitly check that header-provided dimensions are greater than zero before use.
