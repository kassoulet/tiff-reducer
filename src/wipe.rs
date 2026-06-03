//! Wipe support: replace pixel data with synthetic, highly compressible data.
//!
//! The synthetic data is simply the original sample values sorted per channel.
//! Sorting is a permutation of the pixels, so the per-channel histogram — and
//! therefore min/max/mean and every other position-independent statistic — is
//! preserved exactly. The actual image content is destroyed, and the monotonic
//! result compresses extremely well (long runs of identical values).
//!
//! Correctness never depends on the sort order: ANY permutation preserves the
//! histogram. Numerical ordering is only used to maximize compressibility by
//! grouping identical values and minimizing predictor deltas.

use crate::ffi::{SAMPLEFORMAT_IEEEFP, SAMPLEFORMAT_INT};

/// Sort a macro-generated sample type in place inside a raw byte buffer.
macro_rules! sort_typed {
    ($buf:expr, $t:ty) => {{
        let size = std::mem::size_of::<$t>();
        let mut values: Vec<$t> = $buf
            .chunks_exact(size)
            .map(|c| <$t>::from_ne_bytes(c.try_into().unwrap()))
            .collect();
        values.sort_unstable();
        for (chunk, v) in $buf.chunks_exact_mut(size).zip(values) {
            chunk.copy_from_slice(&v.to_ne_bytes());
        }
    }};
}

/// Sort a float sample type in place inside a raw byte buffer (total order).
macro_rules! sort_typed_float {
    ($buf:expr, $t:ty) => {{
        let size = std::mem::size_of::<$t>();
        let mut values: Vec<$t> = $buf
            .chunks_exact(size)
            .map(|c| <$t>::from_ne_bytes(c.try_into().unwrap()))
            .collect();
        values.sort_unstable_by(|a, b| a.total_cmp(b));
        for (chunk, v) in $buf.chunks_exact_mut(size).zip(values) {
            chunk.copy_from_slice(&v.to_ne_bytes());
        }
    }};
}

/// Sort the samples of a single channel in place.
///
/// `buf` holds contiguous samples of one channel in native byte order
/// (as returned by libtiff). Sub-byte bit depths (1/2/4-bit packed) are
/// sorted at the byte level, which still preserves the multiset of packed
/// values for byte-aligned rows.
pub fn sort_samples(buf: &mut [u8], bps: u16, fmt: u16) {
    match (bps, fmt) {
        (16, SAMPLEFORMAT_INT) => sort_typed!(buf, i16),
        (16, SAMPLEFORMAT_IEEEFP) => sort_typed!(buf, u16), // half: bit-pattern order is fine
        (16, _) => sort_typed!(buf, u16),
        (32, SAMPLEFORMAT_IEEEFP) => sort_typed_float!(buf, f32),
        (32, SAMPLEFORMAT_INT) => sort_typed!(buf, i32),
        (32, _) => sort_typed!(buf, u32),
        (64, SAMPLEFORMAT_IEEEFP) => sort_typed_float!(buf, f64),
        (64, SAMPLEFORMAT_INT) => sort_typed!(buf, i64),
        (64, _) => sort_typed!(buf, u64),
        // 8-bit and packed sub-byte depths: byte-level sort
        _ => buf.sort_unstable(),
    }
}

/// Wipe an image buffer in place, preserving the per-channel histogram.
///
/// `spp` is the number of interleaved channels in `buf` (1 for grayscale or
/// for one plane of PLANARCONFIG_SEPARATE data). Channels are de-interleaved,
/// sorted independently, and re-interleaved so each channel keeps its own
/// histogram.
pub fn wipe_buffer(buf: &mut [u8], spp: usize, bps: u16, fmt: u16) {
    if spp <= 1 || bps < 8 {
        // Single channel (or packed sub-byte data, where channel-striding
        // is not byte-addressable): sort the whole buffer.
        sort_samples(buf, bps, fmt);
        return;
    }

    let bytes_per_sample = (bps as usize).div_ceil(8);
    let pixel_stride = bytes_per_sample * spp;
    let num_pixels = buf.len() / pixel_stride;

    let mut channel = vec![0u8; num_pixels * bytes_per_sample];
    for c in 0..spp {
        // Gather channel c
        for p in 0..num_pixels {
            let src = p * pixel_stride + c * bytes_per_sample;
            let dst = p * bytes_per_sample;
            channel[dst..dst + bytes_per_sample].copy_from_slice(&buf[src..src + bytes_per_sample]);
        }
        sort_samples(&mut channel, bps, fmt);
        // Scatter back
        for p in 0..num_pixels {
            let src = p * bytes_per_sample;
            let dst = p * pixel_stride + c * bytes_per_sample;
            buf[dst..dst + bytes_per_sample].copy_from_slice(&channel[src..src + bytes_per_sample]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::SAMPLEFORMAT_UINT;

    #[test]
    fn sort_u8_preserves_histogram() {
        let original: Vec<u8> = vec![42, 0, 255, 7, 7, 128, 0, 200];
        let mut wiped = original.clone();
        sort_samples(&mut wiped, 8, SAMPLEFORMAT_UINT);

        let mut expected = original;
        expected.sort_unstable();
        assert_eq!(wiped, expected);
    }

    #[test]
    fn sort_u16_preserves_histogram() {
        let original: Vec<u16> = vec![1000, 0, 65535, 42, 42, 30000];
        let mut bytes: Vec<u8> = original.iter().flat_map(|v| v.to_ne_bytes()).collect();
        sort_samples(&mut bytes, 16, SAMPLEFORMAT_UINT);

        let wiped: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_ne_bytes([c[0], c[1]]))
            .collect();
        let mut expected = original;
        expected.sort_unstable();
        assert_eq!(wiped, expected);
    }

    #[test]
    fn sort_f32_preserves_histogram_and_stats() {
        let original: Vec<f32> = vec![3.5, -1.0, 0.0, 100.25, -1.0, 7.125];
        let mut bytes: Vec<u8> = original.iter().flat_map(|v| v.to_ne_bytes()).collect();
        sort_samples(&mut bytes, 32, SAMPLEFORMAT_IEEEFP);

        let wiped: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_ne_bytes(c.try_into().unwrap()))
            .collect();

        let mut expected = original.clone();
        expected.sort_unstable_by(|a, b| a.total_cmp(b));
        assert_eq!(wiped, expected);

        // min/max/sum (and thus mean) are exactly preserved
        let sum_orig: f32 = original.iter().sum();
        let sum_wiped: f32 = wiped.iter().sum();
        assert_eq!(wiped.first(), original.iter().min_by(|a, b| a.total_cmp(b)));
        assert_eq!(wiped.last(), original.iter().max_by(|a, b| a.total_cmp(b)));
        assert!((sum_orig - sum_wiped).abs() < 1e-3);
    }

    #[test]
    fn wipe_interleaved_preserves_per_channel_histogram() {
        // RGB pixels: R={10,30,20}, G={100,90,110}, B={5,5,200}
        let original: Vec<u8> = vec![10, 100, 5, 30, 90, 5, 20, 110, 200];
        let mut wiped = original.clone();
        wipe_buffer(&mut wiped, 3, 8, SAMPLEFORMAT_UINT);

        for c in 0..3 {
            let mut orig_ch: Vec<u8> = original.iter().skip(c).step_by(3).copied().collect();
            let mut wiped_ch: Vec<u8> = wiped.iter().skip(c).step_by(3).copied().collect();
            orig_ch.sort_unstable();
            // Wiped channel is already sorted; verify same multiset
            assert_eq!(wiped_ch, orig_ch);
            wiped_ch.sort_unstable();
            assert_eq!(wiped_ch, orig_ch);
        }
    }
}
