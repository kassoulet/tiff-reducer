use crate::ffi::*;
use libc::{c_char, c_void};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

// Static GeoTIFF tag names (null-terminated)
static MODELPIXELSCALE_NAME: &[u8] = b"ModelPixelScaleTag\0";
static MODELTIEPOINT_NAME: &[u8] = b"ModelTiepointTag\0";
static GEOKEYDIRECTORY_NAME: &[u8] = b"GeoKeyDirectoryTag\0";
static GEODOUBLEPARAMS_NAME: &[u8] = b"GeoDoubleParamsTag\0";
static GEOASCII_NAME: &[u8] = b"GeoAsciiParamsTag\0";

/// GeoTIFF tag data extracted from raw file
pub struct GeoTiffData {
    pub model_pixel_scale: Option<Vec<f64>>,
    pub model_tiepoint: Option<Vec<f64>>,
    pub geokey_directory: Option<Vec<u16>>,
    pub geo_double_params: Option<Vec<f64>>,
    pub geo_ascii_params: Option<String>,
}

impl GeoTiffData {
    pub fn new() -> Self {
        GeoTiffData {
            model_pixel_scale: None,
            model_tiepoint: None,
            geokey_directory: None,
            geo_double_params: None,
            geo_ascii_params: None,
        }
    }
}

/// Read GeoTIFF tags from raw TIFF file
pub fn read_geotiff_from_file(path: &std::path::Path) -> std::io::Result<GeoTiffData> {
    let mut file = File::open(path)?;
    let mut result = GeoTiffData::new();

    // Read TIFF header to determine endianness and IFD offset
    let mut header = [0u8; 8];
    file.read_exact(&mut header)?;

    let (little_endian, ifd_offset) = if header[0] == 0x49 && header[1] == 0x49 {
        // Little endian (II)
        (true, u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64)
    } else if header[0] == 0x4D && header[1] == 0x4D {
        // Big endian (MM)
        (false, u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as u64)
    } else {
        return Ok(result); // Not a valid TIFF file
    };

    // Read IFD entries
    file.seek(SeekFrom::Start(ifd_offset))?;
    let mut dir_count = [0u8; 2];
    file.read_exact(&mut dir_count)?;

    let num_entries = if little_endian {
        u16::from_le_bytes([dir_count[0], dir_count[1]])
    } else {
        u16::from_be_bytes([dir_count[0], dir_count[1]])
    } as usize;

    // GeoTIFF tags we're looking for
    const GEOTIFF_TAGS: [u32; 5] = [33550, 33922, 34735, 34736, 34737];

    for _ in 0..num_entries {
        let mut entry = [0u8; 12];
        file.read_exact(&mut entry)?;

        let tag = if little_endian {
            u16::from_le_bytes([entry[0], entry[1]]) as u32
        } else {
            u16::from_be_bytes([entry[0], entry[1]]) as u32
        };

        let typ = if little_endian {
            u16::from_le_bytes([entry[2], entry[3]])
        } else {
            u16::from_be_bytes([entry[2], entry[3]])
        };

        let count = if little_endian {
            u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]])
        } else {
            u32::from_be_bytes([entry[4], entry[5], entry[6], entry[7]])
        };

        let value_offset = if little_endian {
            u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]])
        } else {
            u32::from_be_bytes([entry[8], entry[9], entry[10], entry[11]])
        };

        // Check if this is a GeoTIFF tag
        if GEOTIFF_TAGS.contains(&tag) {
            let type_size = match typ {
                1 => 1,  // BYTE
                2 => 1,  // ASCII
                3 => 2,  // SHORT
                4 => 4,  // LONG
                5 => 8,  // RATIONAL
                9 => 4,  // SLONG
                11 => 4, // FLOAT
                12 => 8, // DOUBLE
                _ => 0,
            };

            let data_size = (count as usize) * (type_size as usize);
            let data_pos = if data_size <= 4 {
                // Value is stored inline in the entry
                8 + 4
            } else {
                // Value is at offset
                value_offset as u64
            };

            match tag {
                33550 => {
                    // ModelPixelScaleTag - array of doubles
                    file.seek(SeekFrom::Start(data_pos))?;
                    let mut data = vec![0f64; count as usize];
                    for val in &mut data {
                        let mut buf = [0u8; 8];
                        file.read_exact(&mut buf)?;
                        *val = if little_endian {
                            f64::from_le_bytes(buf)
                        } else {
                            f64::from_be_bytes(buf)
                        };
                    }
                    result.model_pixel_scale = Some(data);
                }
                33922 => {
                    // ModelTiepointTag - array of doubles
                    file.seek(SeekFrom::Start(data_pos))?;
                    let mut data = vec![0f64; count as usize];
                    for val in &mut data {
                        let mut buf = [0u8; 8];
                        file.read_exact(&mut buf)?;
                        *val = if little_endian {
                            f64::from_le_bytes(buf)
                        } else {
                            f64::from_be_bytes(buf)
                        };
                    }
                    result.model_tiepoint = Some(data);
                }
                34735 => {
                    // GeoKeyDirectoryTag - array of shorts
                    file.seek(SeekFrom::Start(data_pos))?;
                    let mut data = vec![0u16; count as usize];
                    for val in &mut data {
                        let mut buf = [0u8; 2];
                        file.read_exact(&mut buf)?;
                        *val = if little_endian {
                            u16::from_le_bytes(buf)
                        } else {
                            u16::from_be_bytes(buf)
                        };
                    }
                    result.geokey_directory = Some(data);
                }
                34736 => {
                    // GeoDoubleParamsTag - array of doubles
                    file.seek(SeekFrom::Start(data_pos))?;
                    let mut data = vec![0f64; count as usize];
                    for val in &mut data {
                        let mut buf = [0u8; 8];
                        file.read_exact(&mut buf)?;
                        *val = if little_endian {
                            f64::from_le_bytes(buf)
                        } else {
                            f64::from_be_bytes(buf)
                        };
                    }
                    result.geo_double_params = Some(data);
                }
                34737 => {
                    // GeoAsciiParamsTag - ASCII string
                    file.seek(SeekFrom::Start(data_pos))?;
                    let mut data = vec![0u8; count as usize];
                    file.read_exact(&mut data)?;
                    result.geo_ascii_params = Some(String::from_utf8_lossy(&data).to_string());
                }
                _ => {}
            }
        }
    }

    Ok(result)
}

pub unsafe fn clone_metadata(src: *mut TIFF, dst: *mut TIFF, geotiff_data: &GeoTiffData) {
    copy_tag_u32(src, dst, TIFFTAG_IMAGEWIDTH);
    copy_tag_u32(src, dst, TIFFTAG_IMAGELENGTH);
    copy_tag_u16(src, dst, TIFFTAG_BITSPERSAMPLE);
    copy_tag_u16(src, dst, TIFFTAG_SAMPLESPERPIXEL);
    copy_tag_u16(src, dst, TIFFTAG_PHOTOMETRIC);
    copy_tag_u16(src, dst, TIFFTAG_PLANARCONFIG);
    copy_tag_u16(src, dst, TIFFTAG_SAMPLEFORMAT);
    copy_extrasamples(src, dst);

    copy_tag_float(src, dst, TIFFTAG_XRESOLUTION);
    copy_tag_float(src, dst, TIFFTAG_YRESOLUTION);
    copy_tag_u16(src, dst, TIFFTAG_RESOLUTIONUNIT);

    // Copy colormap (palette) if present
    copy_colormap(src, dst);

    // Copy GeoTIFF tags using raw data read from file
    copy_geotiff_tags(src, dst, geotiff_data);
}

/// Copy colormap (palette) from source to destination
pub unsafe fn copy_colormap(src: *mut TIFF, dst: *mut TIFF) {
    let mut rmap: *mut u16 = std::ptr::null_mut();
    let mut gmap: *mut u16 = std::ptr::null_mut();
    let mut bmap: *mut u16 = std::ptr::null_mut();

    // TIFFGetField for colormap returns 3 pointers
    if TIFFGetField(src, TIFFTAG_COLORMAP, &mut rmap, &mut gmap, &mut bmap) != 0 {
        if !rmap.is_null() && !gmap.is_null() && !bmap.is_null() {
            // Colormap has 2^16 entries for 16-bit colormap (even for 8-bit images)
            TIFFSetField(dst, TIFFTAG_COLORMAP, rmap, gmap, bmap);
        }
    }
}

/// Copy ExtraSamples tag for alpha channel preservation
pub unsafe fn copy_extrasamples(src: *mut TIFF, dst: *mut TIFF) {
    let mut extra_samples: *mut u16 = std::ptr::null_mut();
    let mut count: u16 = 0;

    // TIFFGetField for ExtraSamples returns count and pointer to array
    if TIFFGetField(src, TIFFTAG_EXTRASAMPLES, &mut count, &mut extra_samples) != 0 {
        if !extra_samples.is_null() && count > 0 {
            TIFFSetField(dst, TIFFTAG_EXTRASAMPLES, count as u32, extra_samples);
        }
    }
}

/// Copy ICC color profile from source to destination
pub unsafe fn copy_icc_profile(src: *mut TIFF, dst: *mut TIFF) {
    let mut profile: *mut u8 = std::ptr::null_mut();
    let mut count: u32 = 0;

    // TIFFGetField for ICC profile returns count and pointer to byte array
    if TIFFGetField(src, TIFFTAG_ICCPROFILE, &mut count, &mut profile) != 0 {
        if !profile.is_null() && count > 0 {
            TIFFSetField(dst, TIFFTAG_ICCPROFILE, count, profile);
        }
    }
}

/// Public FFI version - registers GeoTIFF tags for reading/writing
/// Must be called immediately after opening a TIFF file
pub unsafe fn register_geotiff_tags_ffi(tif: *mut TIFF) {
    register_geotiff_tags(tif);
    // Note: Compression level tags are handled by libtiff 4.7+ internally
    // No registration needed for TIFFTAG_ZSTDLEVEL, TIFFTAG_DEFLATELEVEL, etc.
}

unsafe fn register_geotiff_tags(tif: *mut TIFF) {
    // Define GeoTIFF tag field info structures with static string pointers
    // Note: field_readcount/writecount: -1 = VARIABLE, -2 = ARRAY, -3 = CUSTOM
    let geotiff_field_info: [TIFFFieldInfo; 5] = [
        TIFFFieldInfo {
            field_tag: TIFFTAG_MODELPIXELSCALETAG,
            field_readcount: -2,  // VARIABLE2 - array with count passed
            field_writecount: -2,
            field_type: TIFF_TYPE_DOUBLE,
            field_oktochange: 1,
            field_passcount: 1,  // count is passed
            field_name: MODELPIXELSCALE_NAME.as_ptr() as *const c_char,
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_MODELTIEPOINTTAG,
            field_readcount: -2,
            field_writecount: -2,
            field_type: TIFF_TYPE_DOUBLE,
            field_oktochange: 1,
            field_passcount: 1,
            field_name: MODELTIEPOINT_NAME.as_ptr() as *const c_char,
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_GEOKEYDIRECTORYTAG,
            field_readcount: -2,
            field_writecount: -2,
            field_type: TIFF_TYPE_SHORT,
            field_oktochange: 1,
            field_passcount: 1,
            field_name: GEOKEYDIRECTORY_NAME.as_ptr() as *const c_char,
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_GEODOUBLEPARAMSTAG,
            field_readcount: -2,
            field_writecount: -2,
            field_type: TIFF_TYPE_DOUBLE,
            field_oktochange: 1,
            field_passcount: 1,
            field_name: GEODOUBLEPARAMS_NAME.as_ptr() as *const c_char,
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_GEOASCIIPARAMSTAG,
            field_readcount: -1,  // VARIABLE - null-terminated string
            field_writecount: -1,
            field_type: TIFF_TYPE_ASCII,
            field_oktochange: 1,
            field_passcount: 0,
            field_name: GEOASCII_NAME.as_ptr() as *const c_char,
        },
    ];

    TIFFMergeFieldInfo(tif, geotiff_field_info.as_ptr(), 5);
}

unsafe fn copy_tag_u32(src: *mut TIFF, dst: *mut TIFF, tag: u32) {
    let mut val: u32 = 0;
    if TIFFGetField(src, tag, &mut val) != 0 {
        TIFFSetField(dst, tag, val);
    }
}

unsafe fn copy_tag_u16(src: *mut TIFF, dst: *mut TIFF, tag: u32) {
    let mut val: u16 = 0;
    if TIFFGetField(src, tag, &mut val) != 0 {
        TIFFSetField(dst, tag, val as u32);
    }
}

unsafe fn copy_tag_float(src: *mut TIFF, dst: *mut TIFF, tag: u32) {
    let mut val: f32 = 0.0;
    if TIFFGetField(src, tag, &mut val) != 0 {
        TIFFSetField(dst, tag, val as f64);
    }
}

unsafe fn copy_geotiff_tags(src: *mut TIFF, dst: *mut TIFF, geotiff_data: &GeoTiffData) {
    // ModelPixelScaleTag (33550) - array of doubles
    if let Some(ref data) = geotiff_data.model_pixel_scale {
        TIFFSetField(dst, TIFFTAG_MODELPIXELSCALETAG, data.len() as u32, data.as_ptr());
    }

    // ModelTiepointTag (33922) - array of doubles
    if let Some(ref data) = geotiff_data.model_tiepoint {
        TIFFSetField(dst, TIFFTAG_MODELTIEPOINTTAG, data.len() as u32, data.as_ptr());
    }

    // GeoKeyDirectoryTag (34735) - array of shorts
    if let Some(ref data) = geotiff_data.geokey_directory {
        TIFFSetField(dst, TIFFTAG_GEOKEYDIRECTORYTAG, data.len() as u32, data.as_ptr());
    }

    // GeoDoubleParamsTag (34736) - array of doubles
    if let Some(ref data) = geotiff_data.geo_double_params {
        TIFFSetField(dst, TIFFTAG_GEODOUBLEPARAMSTAG, data.len() as u32, data.as_ptr());
    }

    // GeoAsciiParamsTag (34737) - ASCII string
    if let Some(ref data) = geotiff_data.geo_ascii_params {
        let c_str = std::ffi::CString::new(data.as_str()).unwrap();
        TIFFSetField(dst, TIFFTAG_GEOASCIIPARAMSTAG, c_str.as_ptr());
    }
}
