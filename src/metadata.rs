#![allow(clippy::collapsible_if, dead_code)]

use crate::ffi::*;
use anyhow::{anyhow, Result};
use libc::c_char;
use std::sync::OnceLock;

/// Read and clone ALL metadata from source to destination
/// This function clones all non-conflicting metadata.
///
/// GeoTIFF (33550, 33922, 34735, 34736, 34737) and GDAL (42112, 42113) tags
/// rely on the definitions registered by `install_tag_extender()`.
pub unsafe fn clone_metadata(src: *mut TIFF, dst: *mut TIFF) -> Result<()> {
    // Resolution and units
    copy_tag_float(src, dst, TIFFTAG_XRESOLUTION)?;
    copy_tag_float(src, dst, TIFFTAG_YRESOLUTION)?;
    copy_tag_u16(src, dst, TIFFTAG_RESOLUTIONUNIT)?;

    // Orientation
    copy_tag_u16(src, dst, TIFFTAG_ORIENTATION)?;

    // NewSubfileType (reduced-resolution / mask flags)
    copy_tag_u32(src, dst, TIFFTAG_SUBFILETYPE)?;

    // FillOrder
    copy_tag_u16(src, dst, TIFFTAG_FILLORDER)?;

    // Common ASCII tags
    copy_tag_ascii(src, dst, TIFFTAG_MAKE)?;
    copy_tag_ascii(src, dst, TIFFTAG_MODEL)?;
    copy_tag_ascii(src, dst, TIFFTAG_SOFTWARE)?;
    copy_tag_ascii(src, dst, TIFFTAG_DATETIME)?;
    copy_tag_ascii(src, dst, TIFFTAG_ARTIST)?;
    copy_tag_ascii(src, dst, TIFFTAG_COPYRIGHT)?;

    // Specialized metadata components
    copy_extrasamples(src, dst)?;
    copy_colormap(src, dst)?;
    copy_geotiff_tags(src, dst)?;
    copy_gdal_tags(src, dst)?;
    copy_icc_profile(src, dst)?;
    copy_ycbcr_tags(src, dst)?;
    copy_cmyk_tags(src, dst)?;
    copy_image_description(src, dst)?;
    Ok(())
}

/// Copy colormap (palette) from source to destination
pub unsafe fn copy_colormap(src: *mut TIFF, dst: *mut TIFF) -> Result<()> {
    let mut rmap: *mut u16 = std::ptr::null_mut();
    let mut gmap: *mut u16 = std::ptr::null_mut();
    let mut bmap: *mut u16 = std::ptr::null_mut();

    // TIFFGetField for colormap returns 3 pointers
    if TIFFGetField(src, TIFFTAG_COLORMAP, &mut rmap, &mut gmap, &mut bmap) != 0 {
        if !rmap.is_null() && !gmap.is_null() && !bmap.is_null() {
            // Colormap has 2^16 entries for 16-bit colormap (even for 8-bit images)
            if TIFFSetField(dst, TIFFTAG_COLORMAP, rmap, gmap, bmap) == 0 {
                return Err(anyhow!("Failed to set colormap"));
            }
        }
    }
    Ok(())
}

/// Copy ExtraSamples tag for alpha channel preservation
pub unsafe fn copy_extrasamples(src: *mut TIFF, dst: *mut TIFF) -> Result<()> {
    let mut extra_samples: *mut u16 = std::ptr::null_mut();
    let mut count: u16 = 0;

    // TIFFGetField for ExtraSamples returns count and pointer to array
    if TIFFGetField(src, TIFFTAG_EXTRASAMPLES, &mut count, &mut extra_samples) != 0 {
        if !extra_samples.is_null() && count > 0 {
            // Safety: cap count to prevent massive allocations if libtiff returns garbage
            let safe_count = std::cmp::min(count as usize, 1024);
            if TIFFSetField(dst, TIFFTAG_EXTRASAMPLES, safe_count as u32, extra_samples) == 0 {
                return Err(anyhow!("Failed to set ExtraSamples tag"));
            }
        }
    }
    Ok(())
}

/// Copy ICC color profile from source to destination
pub unsafe fn copy_icc_profile(src: *mut TIFF, dst: *mut TIFF) -> Result<()> {
    let mut profile: *mut u8 = std::ptr::null_mut();
    let mut count: u32 = 0;

    // TIFFGetField for ICC profile returns count and pointer to byte array
    if TIFFGetField(src, TIFFTAG_ICCPROFILE, &mut count, &mut profile) != 0 {
        if !profile.is_null() && count > 0 && count < 100 * 1024 * 1024 {
            // 100MB limit
            if TIFFSetField(dst, TIFFTAG_ICCPROFILE, count, profile) == 0 {
                return Err(anyhow!("Failed to set ICC profile"));
            }
        }
    }
    Ok(())
}

/// Copy YCbCr color space tags
/// Only call this when the destination is also YCbCr (not when converting to RGB)
pub unsafe fn copy_ycbcr_tags(src: *mut TIFF, dst: *mut TIFF) -> Result<()> {
    // YCbCrSubsampling (two SHORT values: horizontal, vertical)
    let mut h_sub: u16 = 0;
    let mut v_sub: u16 = 0;
    if TIFFGetField(src, TIFFTAG_YCBCRSUBSAMPLING, &mut h_sub, &mut v_sub) != 0 {
        if TIFFSetField(dst, TIFFTAG_YCBCRSUBSAMPLING, h_sub as u32, v_sub as u32) == 0 {
            return Err(anyhow!("Failed to set YCbCr subsampling"));
        }
    }

    // YCbCrPositioning (single SHORT value)
    let mut positioning: u16 = 0;
    if TIFFGetField(src, TIFFTAG_YCBCRPOSITION, &mut positioning) != 0 {
        if TIFFSetField(dst, TIFFTAG_YCBCRPOSITION, positioning as u32) == 0 {
            return Err(anyhow!("Failed to set YCbCr positioning"));
        }
    }

    // YCbCrCoefficients (3 values) and ReferenceBlackWhite (6 values): libtiff
    // passes both as a single `float*` (TIFF_SETGET_C0_FLOAT), not as varargs
    copy_tag_float_array(src, dst, TIFFTAG_YCBCRCOEFFICIENTS)?;
    copy_tag_float_array(src, dst, TIFFTAG_REFERENCEBLACKWHITE)?;
    Ok(())
}

/// Copy a fixed-count float array tag, which libtiff gets and sets as one `float*`.
unsafe fn copy_tag_float_array(src: *mut TIFF, dst: *mut TIFF, tag: u32) -> Result<()> {
    let mut values: *mut f32 = std::ptr::null_mut();
    if TIFFGetField(src, tag, &mut values) != 0 && !values.is_null() {
        if TIFFSetField(dst, tag, values) == 0 {
            return Err(anyhow!("Failed to set float array tag {}", tag));
        }
    }
    Ok(())
}

/// Copy YCbCr color space tags (early version called before compression setup)
/// Only call this when the destination is also YCbCr (not when converting to RGB)
pub unsafe fn copy_cmyk_tags(src: *mut TIFF, dst: *mut TIFF) -> Result<()> {
    // InkSet (single SHORT value)
    let mut inkset: u16 = 0;
    if TIFFGetField(src, TIFFTAG_INKSET, &mut inkset) != 0 {
        if TIFFSetField(dst, TIFFTAG_INKSET, inkset as u32) == 0 {
            return Err(anyhow!("Failed to set InkSet"));
        }
    }

    // NumberOfInks (single SHORT value)
    let mut num_inks: u16 = 0;
    if TIFFGetField(src, TIFFTAG_NUMBEROFINKS, &mut num_inks) != 0 {
        if TIFFSetField(dst, TIFFTAG_NUMBEROFINKS, num_inks as u32) == 0 {
            return Err(anyhow!("Failed to set NumberOfInks"));
        }
    }

    // DotRange (two SHORT values)
    let mut dot_min: u16 = 0;
    let mut dot_max: u16 = 0;
    if TIFFGetField(src, TIFFTAG_DOTRANGE, &mut dot_min, &mut dot_max) != 0 {
        if TIFFSetField(dst, TIFFTAG_DOTRANGE, dot_min as u32, dot_max as u32) == 0 {
            return Err(anyhow!("Failed to set DotRange"));
        }
    }

    // InkNames (ASCII string)
    let mut names: *mut c_char = std::ptr::null_mut();
    if TIFFGetField(src, TIFFTAG_INKNAMES, &mut names) != 0 {
        if !names.is_null() {
            if TIFFSetField(dst, TIFFTAG_INKNAMES, names) == 0 {
                return Err(anyhow!("Failed to set InkNames"));
            }
        }
    }
    Ok(())
}

/// Copy ImageDescription tag
pub unsafe fn copy_image_description(src: *mut TIFF, dst: *mut TIFF) -> Result<()> {
    let mut desc: *mut c_char = std::ptr::null_mut();
    if TIFFGetField(src, TIFFTAG_IMAGEDESCRIPTION, &mut desc) != 0 {
        if !desc.is_null() {
            if TIFFSetField(dst, TIFFTAG_IMAGEDESCRIPTION, desc) == 0 {
                return Err(anyhow!("Failed to set ImageDescription"));
            }
        }
    }
    Ok(())
}

/// Copy the GDAL metadata (XML) and NoData tags.
///
/// Both are ASCII tags; copying the NoData text verbatim keeps GDAL's exact
/// spelling ("nan", "-3.4028234663852886e+38", ...). Requires the tag extender.
pub unsafe fn copy_gdal_tags(src: *mut TIFF, dst: *mut TIFF) -> Result<()> {
    copy_tag_ascii(src, dst, TIFFTAG_GDAL_NODATA)?;
    copy_tag_ascii(src, dst, TIFFTAG_GDAL_METADATA)
}

/// Previously installed tag extender, chained from ours.
static PREVIOUS_EXTENDER: OnceLock<Option<TIFFExtendProc>> = OnceLock::new();

/// Install a libtiff tag extender registering the GeoTIFF and GDAL tags on
/// every handle. Call once at startup, before opening any file.
///
/// The extender runs inside `TIFFDefaultDirectory`, i.e. *before* a directory
/// is parsed. Merging field info after `TIFFOpen` (as this code used to) is
/// too late for the source handle: libtiff has already given unknown tags an
/// anonymous definition (passcount = 1), which then wins over ours.
pub fn install_tag_extender() {
    PREVIOUS_EXTENDER.get_or_init(|| unsafe { TIFFSetTagExtender(Some(tag_extender)) });
}

unsafe extern "C" fn tag_extender(tif: *mut TIFF) {
    register_custom_tags(tif);
    if let Some(Some(previous)) = PREVIOUS_EXTENDER.get() {
        previous(tif);
    }
}

/// Register GeoTIFF and GDAL tag definitions on `tif`.
unsafe fn register_custom_tags(tif: *mut TIFF) {
    struct SyncFieldInfo([TIFFFieldInfo; 7]);
    unsafe impl Sync for SyncFieldInfo {}

    static CUSTOM_FIELDS: SyncFieldInfo = SyncFieldInfo([
        TIFFFieldInfo {
            field_tag: TIFFTAG_MODELPIXELSCALETAG,
            field_readcount: TIFF_VARIABLE2,
            field_writecount: TIFF_VARIABLE2,
            field_type: TIFF_DOUBLE,
            field_bit: FIELD_CUSTOM,
            field_oktochange: 1,
            field_passcount: 1,
            field_name: c"ModelPixelScaleTag".as_ptr(),
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_MODELTIEPOINTTAG,
            field_readcount: TIFF_VARIABLE2,
            field_writecount: TIFF_VARIABLE2,
            field_type: TIFF_DOUBLE,
            field_bit: FIELD_CUSTOM,
            field_oktochange: 1,
            field_passcount: 1,
            field_name: c"ModelTiepointTag".as_ptr(),
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_GEOKEYDIRECTORYTAG,
            field_readcount: TIFF_VARIABLE2,
            field_writecount: TIFF_VARIABLE2,
            field_type: TIFF_SHORT,
            field_bit: FIELD_CUSTOM,
            field_oktochange: 0,
            field_passcount: 1,
            field_name: c"GeoKeyDirectoryTag".as_ptr(),
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_GEODOUBLEPARAMSTAG,
            field_readcount: TIFF_VARIABLE2,
            field_writecount: TIFF_VARIABLE2,
            field_type: TIFF_DOUBLE,
            field_bit: FIELD_CUSTOM,
            field_oktochange: 0,
            field_passcount: 1,
            field_name: c"GeoDoubleParamsTag".as_ptr(),
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_GEOASCIIPARAMSTAG,
            field_readcount: TIFF_VARIABLE2,
            field_writecount: TIFF_VARIABLE2,
            field_type: TIFF_ASCII,
            field_bit: FIELD_CUSTOM,
            field_oktochange: 0,
            field_passcount: 1,
            field_name: c"GeoAsciiParamsTag".as_ptr(),
        },
        // GDAL tags: plain NUL-terminated strings, no count argument
        TIFFFieldInfo {
            field_tag: TIFFTAG_GDAL_METADATA,
            field_readcount: TIFF_VARIABLE,
            field_writecount: TIFF_VARIABLE,
            field_type: TIFF_ASCII,
            field_bit: FIELD_CUSTOM,
            field_oktochange: 1,
            field_passcount: 0,
            field_name: c"GDALMetadata".as_ptr(),
        },
        TIFFFieldInfo {
            field_tag: TIFFTAG_GDAL_NODATA,
            field_readcount: TIFF_VARIABLE,
            field_writecount: TIFF_VARIABLE,
            field_type: TIFF_ASCII,
            field_bit: FIELD_CUSTOM,
            field_oktochange: 1,
            field_passcount: 0,
            field_name: c"GDALNoDataValue".as_ptr(),
        },
    ]);

    TIFFMergeFieldInfo(tif, CUSTOM_FIELDS.0.as_ptr(), CUSTOM_FIELDS.0.len() as i32);
}

/// Copy GeoTIFF tags using the registered tag definitions
/// Requires the definitions registered by `install_tag_extender()`
unsafe fn copy_geotiff_tags(src: *mut TIFF, dst: *mut TIFF) -> Result<()> {
    // Copy ModelPixelScaleTag (array of 3 doubles)
    let mut pixel_scale: *mut f64 = std::ptr::null_mut();
    let mut count: u32 = 0;
    if TIFFGetField(
        src,
        TIFFTAG_MODELPIXELSCALETAG,
        &mut count,
        &mut pixel_scale,
    ) != 0
    {
        if !pixel_scale.is_null() && count > 0 && count < 1000 {
            if crate::ffi::TIFFSetField(dst, TIFFTAG_MODELPIXELSCALETAG, count, pixel_scale) == 0 {
                return Err(anyhow!("Failed to set ModelPixelScaleTag"));
            }
        }
    }

    // Copy ModelTiepointTag (array of 6 doubles)
    let mut tiepoints: *mut f64 = std::ptr::null_mut();
    count = 0;
    if TIFFGetField(src, TIFFTAG_MODELTIEPOINTTAG, &mut count, &mut tiepoints) != 0 {
        if !tiepoints.is_null() && count > 0 && count < 1000 {
            if crate::ffi::TIFFSetField(dst, TIFFTAG_MODELTIEPOINTTAG, count, tiepoints) == 0 {
                return Err(anyhow!("Failed to set ModelTiepointTag"));
            }
        }
    }

    // Copy GeoKeyDirectoryTag (array of shorts)
    let mut geo_keys: *mut u16 = std::ptr::null_mut();
    count = 0;
    if TIFFGetField(src, TIFFTAG_GEOKEYDIRECTORYTAG, &mut count, &mut geo_keys) != 0 {
        if !geo_keys.is_null() && count > 0 && count < 10000 {
            if crate::ffi::TIFFSetField(dst, TIFFTAG_GEOKEYDIRECTORYTAG, count, geo_keys) == 0 {
                return Err(anyhow!("Failed to set GeoKeyDirectoryTag"));
            }
        }
    }

    // Copy GeoDoubleParamsTag (array of doubles)
    let mut geo_doubles: *mut f64 = std::ptr::null_mut();
    count = 0;
    if TIFFGetField(
        src,
        TIFFTAG_GEODOUBLEPARAMSTAG,
        &mut count,
        &mut geo_doubles,
    ) != 0
    {
        if !geo_doubles.is_null() && count > 0 && count < 1000 {
            if crate::ffi::TIFFSetField(dst, TIFFTAG_GEODOUBLEPARAMSTAG, count, geo_doubles) == 0 {
                return Err(anyhow!("Failed to set GeoDoubleParamsTag"));
            }
        }
    }

    // Copy GeoAsciiParamsTag (ASCII string)
    let mut geo_ascii: *mut c_char = std::ptr::null_mut();
    count = 0;
    if TIFFGetField(src, TIFFTAG_GEOASCIIPARAMSTAG, &mut count, &mut geo_ascii) != 0 {
        if !geo_ascii.is_null() && count > 0 {
            if crate::ffi::TIFFSetField(dst, TIFFTAG_GEOASCIIPARAMSTAG, count, geo_ascii) == 0 {
                return Err(anyhow!("Failed to set GeoAsciiParamsTag"));
            }
        }
    }
    Ok(())
}

unsafe fn copy_tag_u32(src: *mut TIFF, dst: *mut TIFF, tag: u32) -> Result<()> {
    let mut val: u32 = 0;
    if TIFFGetField(src, tag, &mut val) != 0 {
        if TIFFSetField(dst, tag, val) == 0 {
            return Err(anyhow!("Failed to set u32 tag {}", tag));
        }
    }
    Ok(())
}

unsafe fn copy_tag_u16(src: *mut TIFF, dst: *mut TIFF, tag: u32) -> Result<()> {
    let mut val: u16 = 0;
    if TIFFGetField(src, tag, &mut val) != 0 {
        if TIFFSetField(dst, tag, val as u32) == 0 {
            return Err(anyhow!("Failed to set u16 tag {}", tag));
        }
    }
    Ok(())
}

unsafe fn copy_tag_float(src: *mut TIFF, dst: *mut TIFF, tag: u32) -> Result<()> {
    let mut val: f32 = 0.0;
    if TIFFGetField(src, tag, &mut val) != 0 {
        if TIFFSetField(dst, tag, val as f64) == 0 {
            return Err(anyhow!("Failed to set float tag {}", tag));
        }
    }
    Ok(())
}

unsafe fn copy_tag_ascii(src: *mut TIFF, dst: *mut TIFF, tag: u32) -> Result<()> {
    let mut ptr: *mut c_char = std::ptr::null_mut();
    if TIFFGetField(src, tag, &mut ptr) != 0 {
        if !ptr.is_null() {
            if TIFFSetField(dst, tag, ptr) == 0 {
                return Err(anyhow!("Failed to set ascii tag {}", tag));
            }
        }
    }
    Ok(())
}
