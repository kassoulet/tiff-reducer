#![allow(clippy::collapsible_if, dead_code)]

use crate::ffi::*;
use libc::c_char;

// Static GeoTIFF tag names (null-terminated)
static MODELPIXELSCALE_NAME: &[u8] = b"ModelPixelScaleTag\0";
static MODELTIEPOINT_NAME: &[u8] = b"ModelTiepointTag\0";
static GEOKEYDIRECTORY_NAME: &[u8] = b"GeoKeyDirectoryTag\0";
static GEODOUBLEPARAMS_NAME: &[u8] = b"GeoDoubleParamsTag\0";
static GEOASCII_NAME: &[u8] = b"GeoAsciiParamsTag\0";

/// Read and clone ALL metadata from source to destination
/// This function clones all non-conflicting metadata including GeoTIFF tags
pub unsafe fn clone_metadata(src: *mut TIFF, dst: *mut TIFF) {
    // Resolution and units
    copy_tag_float(src, dst, TIFFTAG_XRESOLUTION);
    copy_tag_float(src, dst, TIFFTAG_YRESOLUTION);
    copy_tag_u16(src, dst, TIFFTAG_RESOLUTIONUNIT);

    // Specialized metadata components
    copy_extrasamples(src, dst);
    copy_colormap(src, dst);
    copy_geotiff_tags(src, dst);
    copy_icc_profile(src, dst);
    copy_ycbcr_tags(src, dst);
    copy_cmyk_tags(src, dst);
    copy_image_description(src, dst);
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
            // Safety: cap count to prevent massive allocations if libtiff returns garbage
            let safe_count = std::cmp::min(count as u32, 1024);
            TIFFSetField(dst, TIFFTAG_EXTRASAMPLES, safe_count, extra_samples);
        }
    }
}

/// Copy ICC color profile from source to destination
pub unsafe fn copy_icc_profile(src: *mut TIFF, dst: *mut TIFF) {
    let mut profile: *mut u8 = std::ptr::null_mut();
    let mut count: u32 = 0;

    // TIFFGetField for ICC profile returns count and pointer to byte array
    if TIFFGetField(src, TIFFTAG_ICCPROFILE, &mut count, &mut profile) != 0 {
        if !profile.is_null() && count > 0 && count < 100 * 1024 * 1024 {
            // 100MB limit
            TIFFSetField(dst, TIFFTAG_ICCPROFILE, count, profile);
        }
    }
}

/// Copy YCbCr color space tags
pub unsafe fn copy_ycbcr_tags(src: *mut TIFF, dst: *mut TIFF) {
    // YCbCrSubsampling (two SHORT values: horizontal, vertical)
    let mut h_sub: u16 = 0;
    let mut v_sub: u16 = 0;
    if TIFFGetField(src, TIFFTAG_YCBCRSUBSAMPLING, &mut h_sub, &mut v_sub) != 0 {
        TIFFSetField(dst, TIFFTAG_YCBCRSUBSAMPLING, h_sub as u32, v_sub as u32);
    }

    // YCbCrPositioning (single SHORT value)
    let mut positioning: u16 = 0;
    if TIFFGetField(src, TIFFTAG_YCBCRPOSITION, &mut positioning) != 0 {
        TIFFSetField(dst, TIFFTAG_YCBCRPOSITION, positioning as u32);
    }

    // YCbCrCoefficients (three FLOAT values)
    let mut coeff_r: f32 = 0.0;
    let mut coeff_g: f32 = 0.0;
    let mut coeff_b: f32 = 0.0;
    if TIFFGetField(
        src,
        TIFFTAG_YCBCRCOEFFICIENTS,
        &mut coeff_r,
        &mut coeff_g,
        &mut coeff_b,
    ) != 0
    {
        TIFFSetField(
            dst,
            TIFFTAG_YCBCRCOEFFICIENTS,
            coeff_r as f64,
            coeff_g as f64,
            coeff_b as f64,
        );
    }
}

/// Copy CMYK/Ink-related tags
pub unsafe fn copy_cmyk_tags(src: *mut TIFF, dst: *mut TIFF) {
    // InkSet (single SHORT value)
    let mut inkset: u16 = 0;
    if TIFFGetField(src, TIFFTAG_INKSET, &mut inkset) != 0 {
        TIFFSetField(dst, TIFFTAG_INKSET, inkset as u32);
    }

    // DotRange (two SHORT values: 0-65535 representing 0.0-100.0%)
    let mut dot0: u16 = 0;
    let mut dot1: u16 = 0;
    if TIFFGetField(src, TIFFTAG_DOTRANGE, &mut dot0, &mut dot1) != 0 {
        TIFFSetField(dst, TIFFTAG_DOTRANGE, dot0 as u32, dot1 as u32);
    }

    // NumberOfInks (single LONG value)
    let mut num_inks: u32 = 0;
    if TIFFGetField(src, TIFFTAG_NUMBEROFINKS, &mut num_inks) != 0 {
        TIFFSetField(dst, TIFFTAG_NUMBEROFINKS, num_inks);
    }

    // InkNames (ASCII string)
    let mut ink_names: *mut c_char = std::ptr::null_mut();
    if TIFFGetField(src, TIFFTAG_INKNAMES, &mut ink_names) != 0 {
        if !ink_names.is_null() {
            TIFFSetField(dst, TIFFTAG_INKNAMES, ink_names);
        }
    }
}

/// Copy ImageDescription tag (used for OME-XML metadata)
pub unsafe fn copy_image_description(src: *mut TIFF, dst: *mut TIFF) {
    let mut desc: *mut c_char = std::ptr::null_mut();
    if TIFFGetField(src, TIFFTAG_IMAGEDESCRIPTION, &mut desc) != 0 {
        if !desc.is_null() {
            TIFFSetField(dst, TIFFTAG_IMAGEDESCRIPTION, desc);
        }
    }
}

/// Registers GeoTIFF tags for reading/writing with libtiff
pub unsafe fn register_geotiff_tags(tif: *mut TIFF) {
    // Define GeoTIFF tag field info structures
    // Note: For fixed-size arrays, we use the actual count instead of VARIABLE
    // ModelPixelScaleTag: 3 doubles, ModelTiepointTag: 6 doubles
    let geotiff_field_info: [TIFFFieldInfo; 5] = [
        TIFFFieldInfo {
            field_tag: TIFFTAG_MODELPIXELSCALETAG,
            field_readcount: 3, // Fixed 3 doubles
            field_writecount: 3,
            field_type: TIFF_TYPE_DOUBLE,
            field_oktochange: 1,
            field_passcount: 0, // Count is fixed, not passed
            field_name: MODELPIXELSCALE_NAME.as_ptr() as *const c_char,
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_MODELTIEPOINTTAG,
            field_readcount: 6, // Fixed 6 doubles
            field_writecount: 6,
            field_type: TIFF_TYPE_DOUBLE,
            field_oktochange: 1,
            field_passcount: 0,
            field_name: MODELTIEPOINT_NAME.as_ptr() as *const c_char,
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_GEOKEYDIRECTORYTAG,
            field_readcount: -3, // VARIABLE3 - variable number of shorts
            field_writecount: -3,
            field_type: TIFF_TYPE_SHORT,
            field_oktochange: 1,
            field_passcount: 1,
            field_name: GEOKEYDIRECTORY_NAME.as_ptr() as *const c_char,
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_GEODOUBLEPARAMSTAG,
            field_readcount: -3, // VARIABLE3 - variable number of doubles
            field_writecount: -3,
            field_type: TIFF_TYPE_DOUBLE,
            field_oktochange: 1,
            field_passcount: 1,
            field_name: GEODOUBLEPARAMS_NAME.as_ptr() as *const c_char,
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_GEOASCIIPARAMSTAG,
            field_readcount: -1, // VARIABLE - null-terminated string
            field_writecount: -1,
            field_type: TIFF_TYPE_ASCII,
            field_oktochange: 1,
            field_passcount: 0,
            field_name: GEOASCII_NAME.as_ptr() as *const c_char,
        },
    ];

    TIFFMergeFieldInfo(tif, geotiff_field_info.as_ptr(), 5);
}

/// Public FFI version - registers GeoTIFF tags for reading/writing
/// Must be called immediately after opening a TIFF file
pub unsafe fn register_geotiff_tags_ffi(tif: *mut TIFF) {
    register_geotiff_tags(tif);
}

/// Copy GeoTIFF tags using libtiff's native API
/// Uses direct tag access for GeoTIFF standard tags
unsafe fn copy_geotiff_tags(src: *mut TIFF, dst: *mut TIFF) {
    // GeoTIFF tag numbers (from GeoTIFF specification)
    const MODELPIXELSCALE: u32 = 33550;
    const MODELTIEPOINT: u32 = 33922;
    const GEOKEYDIRECTORY: u32 = 34735;
    const GEODOUBLEPARAMS: u32 = 34736;
    const GEOASCII: u32 = 34737;

    // Copy ModelPixelScaleTag (array of doubles)
    let mut ps_count: u32 = 0;
    let mut ps_data: *mut f64 = std::ptr::null_mut();
    if TIFFGetField(src, MODELPIXELSCALE, &mut ps_count, &mut ps_data) != 0 {
        if !ps_data.is_null() && ps_count > 0 && ps_count < 32768 {
            TIFFSetField(dst, MODELPIXELSCALE, ps_count, ps_data);
        }
    }

    // Copy ModelTiepointTag (array of doubles)
    let mut tp_count: u32 = 0;
    let mut tp_data: *mut f64 = std::ptr::null_mut();
    if TIFFGetField(src, MODELTIEPOINT, &mut tp_count, &mut tp_data) != 0 {
        if !tp_data.is_null() && tp_count > 0 && tp_count < 32768 {
            TIFFSetField(dst, MODELTIEPOINT, tp_count, tp_data);
        }
    }

    // Copy GeoKeyDirectoryTag (array of shorts)
    let mut gk_count: u32 = 0;
    let mut gk_data: *mut u16 = std::ptr::null_mut();
    if TIFFGetField(src, GEOKEYDIRECTORY, &mut gk_count, &mut gk_data) != 0 {
        if !gk_data.is_null() && gk_count > 0 && gk_count < 32768 {
            TIFFSetField(dst, GEOKEYDIRECTORY, gk_count, gk_data);
        }
    }

    // Copy GeoDoubleParamsTag (array of doubles)
    let mut gp_count: u32 = 0;
    let mut gp_data: *mut f64 = std::ptr::null_mut();
    if TIFFGetField(src, GEODOUBLEPARAMS, &mut gp_count, &mut gp_data) != 0 {
        if !gp_data.is_null() && gp_count > 0 && gp_count < 32768 {
            TIFFSetField(dst, GEODOUBLEPARAMS, gp_count, gp_data);
        }
    }

    // Copy GeoAsciiParamsTag (ASCII string)
    let mut ascii_data: *mut c_char = std::ptr::null_mut();
    if TIFFGetField(src, GEOASCII, &mut ascii_data) != 0 {
        if !ascii_data.is_null() {
            TIFFSetField(dst, GEOASCII, ascii_data);
        }
    }
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
