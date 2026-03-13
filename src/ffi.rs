use libc::{c_char, c_int, c_void};

pub enum TIFF {}

#[repr(C)]
pub struct TIFFFieldInfo {
    pub field_tag: u32,
    pub field_readcount: i32,
    pub field_writecount: i32,
    pub field_type: u32,
    pub field_oktochange: u32,
    pub field_passcount: u32,
    pub field_name: *const c_char,
}

#[link(name = "tiff")]
extern "C" {
    pub fn TIFFOpen(name: *const c_char, mode: *const c_char) -> *mut TIFF;
    pub fn TIFFClose(tif: *mut TIFF);

    pub fn TIFFGetField(tif: *mut TIFF, tag: u32, ...) -> c_int;
    pub fn TIFFSetField(tif: *mut TIFF, tag: u32, ...) -> c_int;

    pub fn TIFFReadScanline(tif: *mut TIFF, buf: *mut c_void, row: u32, sample: u16) -> c_int;
    pub fn TIFFWriteScanline(tif: *mut TIFF, buf: *mut c_void, row: u32, sample: u16) -> c_int;

    pub fn TIFFScanlineSize(tif: *mut TIFF) -> u32;
    pub fn TIFFDefaultStripSize(tif: *mut TIFF, estimate: u32) -> u32;

    pub fn TIFFSetDirectory(tif: *mut TIFF, dir: u16) -> c_int;
    pub fn TIFFWriteDirectory(tif: *mut TIFF) -> c_int;

    pub fn TIFFIsBigTIFF(tif: *mut TIFF) -> c_int;

    // Error handling - use raw pointer for va_list (unstable in stable Rust)
    pub fn TIFFSetWarningHandler(handler: Option<unsafe extern "C" fn(*const c_char, *const c_char, *mut c_void)>);

    // Tag registration for custom/GeoTIFF tags
    pub fn TIFFMergeFieldInfo(tif: *mut TIFF, info: *const TIFFFieldInfo, n: i32) -> c_int;

    // Re-read directory after registering tags
    pub fn TIFFReadDirectory(tif: *mut TIFF) -> c_int;

    // Check if image is tiled
    pub fn TIFFIsTiled(tif: *mut TIFF) -> c_int;
    pub fn TIFFTileSize(tif: *mut TIFF) -> u32;
    pub fn TIFFReadTile(tif: *mut TIFF, tile: u32, x: u32, y: u32, z: u32, sample: u16) -> i32;
    pub fn TIFFWriteTile(tif: *mut TIFF, buf: *mut c_void, tile: u32, x: u32, y: u32, z: u32, sample: u16) -> i32;
    pub fn TIFFNumberOfTiles(tif: *mut TIFF) -> u32;
    pub fn TIFFCheckTile(tif: *mut TIFF, x: u32, y: u32, z: u32, sample: u16) -> c_int;
    pub fn TIFFReadEncodedTile(tif: *mut TIFF, tile: u32, buf: *mut c_void, size: u32) -> i32;
    pub fn TIFFWriteEncodedTile(tif: *mut TIFF, tile: u32, buf: *mut c_void, size: u32) -> i32;
}

// Suppress libtiff warnings
pub unsafe fn suppress_warnings() {
    TIFFSetWarningHandler(None);
}

pub const TIFFTAG_IMAGEWIDTH: u32 = 256;
pub const TIFFTAG_IMAGELENGTH: u32 = 257;
pub const TIFFTAG_BITSPERSAMPLE: u32 = 258;
pub const TIFFTAG_COMPRESSION: u32 = 259;
pub const TIFFTAG_PHOTOMETRIC: u32 = 262;
pub const TIFFTAG_STRIPOFFSETS: u32 = 273;
pub const TIFFTAG_SAMPLESPERPIXEL: u32 = 277;
pub const TIFFTAG_ROWSPERSTRIP: u32 = 278;
pub const TIFFTAG_STRIPBYTECOUNTS: u32 = 279;
pub const TIFFTAG_XRESOLUTION: u32 = 282;
pub const TIFFTAG_YRESOLUTION: u32 = 283;
pub const TIFFTAG_PLANARCONFIG: u32 = 284;
pub const TIFFTAG_RESOLUTIONUNIT: u32 = 296;
pub const TIFFTAG_PREDICTOR: u32 = 317;
pub const TIFFTAG_COLORMAP: u32 = 320;
pub const TIFFTAG_SAMPLEFORMAT: u32 = 339;
pub const TIFFTAG_SMINSAMPLEVALUE: u32 = 340;
pub const TIFFTAG_EXTRASAMPLES: u32 = 338;
pub const TIFFTAG_ICCPROFILE: u32 = 34675;

// ExtraSamples values
pub const EXTRASAMPLE_UNSPECIFIED: u16 = 0;
pub const EXTRASAMPLE_ASSOCALPHA: u16 = 1;  // Associated alpha
pub const EXTRASAMPLE_UNASSALPHA: u16 = 2;  // Unassociated alpha

// GeoTIFF
pub const TIFFTAG_MODELPIXELSCALETAG: u32 = 33550;
pub const TIFFTAG_MODELTIEPOINTTAG: u32 = 33922;
pub const TIFFTAG_GEOKEYDIRECTORYTAG: u32 = 34735;
pub const TIFFTAG_GEODOUBLEPARAMSTAG: u32 = 34736;
pub const TIFFTAG_GEOASCIIPARAMSTAG: u32 = 34737;

// Tile size tags
pub const TIFFTAG_TILEWIDTH: u32 = 322;
pub const TIFFTAG_TILELENGTH: u32 = 323;

// TIFF data types for field registration
pub const TIFF_TYPE_DOUBLE: u32 = 12;
pub const TIFF_TYPE_FLOAT: u32 = 11;
pub const TIFF_TYPE_LONG: u32 = 4;
pub const TIFF_TYPE_SHORT: u32 = 3;
pub const TIFF_TYPE_ASCII: u32 = 2;

// GeoTIFF field info definitions (readcount: -1 = variable, -2 = array)
pub const FIELD_CUSTOM: i32 = -1;

// Compression types
pub const COMPRESSION_NONE: u16 = 1;
pub const COMPRESSION_LZW: u16 = 5;
pub const COMPRESSION_JPEG: u16 = 7;
pub const COMPRESSION_ADOBE_DEFLATE: u16 = 8;
pub const COMPRESSION_LZMA: u16 = 34925;
pub const COMPRESSION_ZSTD: u16 = 50000;
pub const COMPRESSION_WEBP: u16 = 50001;

pub const PREDICTOR_NONE: u16 = 1;
pub const PREDICTOR_HORIZONTAL: u16 = 2;
pub const PREDICTOR_FLOATINGPOINT: u16 = 3;

// Compression level tags (libtiff 4.7+)
pub const TIFFTAG_ZSTD_LEVEL: u32 = 65564;    // ZSTD compression level
pub const TIFFTAG_DEFLATELEVEL: u32 = 320;    // Deflate compression level
pub const TIFFTAG_LZMAPRESET: u32 = 34926;    // LZMA preset level

pub const SAMPLEFORMAT_UINT: u16 = 1;
pub const SAMPLEFORMAT_INT: u16 = 2;
pub const SAMPLEFORMAT_IEEEFP: u16 = 3;
pub const SAMPLEFORMAT_VOID: u16 = 4;
