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
/// This function clones all non-conflicting metadata.
///
/// GeoTIFF tags (33550, 33922, 34735, 34736, 34737) are copied after being
/// registered with libtiff via register_geotiff_tags().
pub unsafe fn clone_metadata(src: *mut TIFF, dst: *mut TIFF) {
    // Resolution and units
    copy_tag_float(src, dst, TIFFTAG_XRESOLUTION);
    copy_tag_float(src, dst, TIFFTAG_YRESOLUTION);
    copy_tag_u16(src, dst, TIFFTAG_RESOLUTIONUNIT);

    // Specialized metadata components
    copy_extrasamples(src, dst);
    copy_colormap(src, dst);
    copy_geotiff_tags(src, dst);
    copy_gdal_tags(src, dst);
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
/// Only call this when the destination is also YCbCr (not when converting to RGB)
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

/// Copy YCbCr color space tags (early version called before compression setup)
/// Only call this when the destination is also YCbCr (not when converting to RGB)
pub unsafe fn copy_ycbcr_tags_early(src: *mut TIFF, dst: *mut TIFF) {
    // Only copy subsampling and coefficients - these are critical for YCbCr encoding
    // YCbCrPositioning can be copied later with other metadata
    let mut h_sub: u16 = 0;
    let mut v_sub: u16 = 0;
    if TIFFGetField(src, TIFFTAG_YCBCRSUBSAMPLING, &mut h_sub, &mut v_sub) != 0 {
        TIFFSetField(dst, TIFFTAG_YCBCRSUBSAMPLING, h_sub as u32, v_sub as u32);
    }

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

/// Copy GDAL metadata tags (NoDataValue and XML metadata)
/// These tags require manual registration with libtiff
/// For now, skip copying as the registration is causing issues
/// TODO: Fix GDAL tag registration with proper libtiff field info structure
pub unsafe fn copy_gdal_tags(_src: *mut TIFF, _dst: *mut TIFF) {
    // GDAL tags are not supported yet - requires proper libtiff field info structure
    // The NoDataValue will be lost, but pixel data is preserved
}

/// Registers GeoTIFF tags for reading/writing with libtiff
/// Note: XTIFFInitialize() is called once at program startup to register all GeoTIFF tags
/// GDAL tags (42112, 42113) are not supported yet - requires proper libtiff field info structure
pub unsafe fn register_geotiff_tags(_tif: *mut TIFF) {
    // GeoTIFF tags are registered globally by XTIFFInitialize()
    // GDAL tags require manual registration but is disabled due to crashes
}

/// Public FFI version - registers GeoTIFF tags for reading/writing
/// Must be called immediately after opening a TIFF file
pub unsafe fn register_geotiff_tags_ffi(tif: *mut TIFF) {
    register_geotiff_tags(tif);
}

/// Copy GeoTIFF tags using the registered tag definitions
/// Requires that register_geotiff_tags() was called on both src and dst TIFF handles
unsafe fn copy_geotiff_tags(src: *mut TIFF, dst: *mut TIFF) {
    // Copy ModelPixelScaleTag (array of 3 doubles)
    let mut pixel_scale: *mut f64 = std::ptr::null_mut();
    let mut count: u32 = 0;
    if TIFFGetField(src, TIFFTAG_MODELPIXELSCALETAG, &mut count, &mut pixel_scale) != 0 {
        if !pixel_scale.is_null() && count > 0 && count < 1000 {
            let _ = crate::ffi::TIFFSetField(dst, TIFFTAG_MODELPIXELSCALETAG, count, pixel_scale);
        }
    }

    // Copy ModelTiepointTag (array of 6 doubles)
    let mut tiepoints: *mut f64 = std::ptr::null_mut();
    count = 0;
    if TIFFGetField(src, TIFFTAG_MODELTIEPOINTTAG, &mut count, &mut tiepoints) != 0 {
        if !tiepoints.is_null() && count > 0 && count < 1000 {
            let _ = crate::ffi::TIFFSetField(dst, TIFFTAG_MODELTIEPOINTTAG, count, tiepoints);
        }
    }

    // Copy GeoKeyDirectoryTag (array of shorts)
    let mut geo_keys: *mut u16 = std::ptr::null_mut();
    count = 0;
    if TIFFGetField(src, TIFFTAG_GEOKEYDIRECTORYTAG, &mut count, &mut geo_keys) != 0 {
        if !geo_keys.is_null() && count > 0 && count < 10000 {
            let _ = crate::ffi::TIFFSetField(dst, TIFFTAG_GEOKEYDIRECTORYTAG, count, geo_keys);
        }
    }

    // Copy GeoDoubleParamsTag (array of doubles)
    let mut geo_doubles: *mut f64 = std::ptr::null_mut();
    count = 0;
    if TIFFGetField(src, TIFFTAG_GEODOUBLEPARAMSTAG, &mut count, &mut geo_doubles) != 0 {
        if !geo_doubles.is_null() && count > 0 && count < 1000 {
            let _ = crate::ffi::TIFFSetField(dst, TIFFTAG_GEODOUBLEPARAMSTAG, count, geo_doubles);
        }
    }

    // Copy GeoAsciiParamsTag (ASCII string)
    let mut geo_ascii: *mut c_char = std::ptr::null_mut();
    if TIFFGetField(src, TIFFTAG_GEOASCIIPARAMSTAG, &mut geo_ascii) != 0 {
        if !geo_ascii.is_null() {
            let _ = crate::ffi::TIFFSetField(dst, TIFFTAG_GEOASCIIPARAMSTAG, geo_ascii);
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
