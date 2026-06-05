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
//!
//! Two strategies are used:
//! - **Histogram streaming** (8/16-bit integers): only per-channel value
//!   counts are accumulated (256 or 65536 buckets); the sorted output is
//!   synthesized directly from the cumulative histogram. O(n) time, O(1)
//!   memory — the image plane is never materialized.
//! - **Parallel sort** (32/64-bit integers, floats): the plane is read into
//!   memory and sorted per channel with rayon.

use crate::ffi::{SAMPLEFORMAT_IEEEFP, SAMPLEFORMAT_INT, SAMPLEFORMAT_UINT};
use rayon::prelude::*;

// ============================================================================
// Histogram streaming (8/16-bit integers)
// ============================================================================

/// Per-channel value histogram for 8/16-bit integer samples.
///
/// Bucket index is the raw bit pattern (`u8`/`u16`); for signed data the
/// emission order is remapped so values are produced in ascending signed
/// order (maximizes compressibility, the histogram itself is unaffected).
pub struct Histogram {
    bps: u16,
    fmt: u16,
    counts: Vec<Vec<u64>>, // [channel][bucket]
}

impl Histogram {
    /// Whether the histogram strategy applies to this sample type
    pub fn supports(bps: u16, fmt: u16) -> bool {
        (bps == 8 || bps == 16) && (fmt == SAMPLEFORMAT_UINT || fmt == SAMPLEFORMAT_INT)
    }

    pub fn new(spp: usize, bps: u16, fmt: u16) -> Self {
        debug_assert!(Self::supports(bps, fmt));
        let buckets = if bps == 8 { 256 } else { 65536 };
        Histogram {
            bps,
            fmt,
            counts: vec![vec![0u64; buckets]; spp.max(1)],
        }
    }

    /// Count all samples in an interleaved slice (whole rows or the valid
    /// region of a tile row). The slice length must be a multiple of the
    /// pixel stride.
    pub fn accumulate(&mut self, buf: &[u8]) {
        let spp = self.counts.len();
        if self.bps == 8 {
            if spp == 1 {
                let c = &mut self.counts[0];
                for &b in buf {
                    c[b as usize] += 1;
                }
            } else {
                for px in buf.chunks_exact(spp) {
                    for (c, &b) in px.iter().enumerate() {
                        self.counts[c][b as usize] += 1;
                    }
                }
            }
        } else if spp == 1 {
            let c = &mut self.counts[0];
            for s in buf.chunks_exact(2) {
                c[u16::from_ne_bytes([s[0], s[1]]) as usize] += 1;
            }
        } else {
            for px in buf.chunks_exact(2 * spp) {
                for (c, s) in px.chunks_exact(2).enumerate() {
                    self.counts[c][u16::from_ne_bytes([s[0], s[1]]) as usize] += 1;
                }
            }
        }
    }

    /// Merge another histogram into this one (for parallel accumulation)
    pub fn merge(mut self, other: Histogram) -> Histogram {
        for (a, b) in self.counts.iter_mut().zip(other.counts) {
            for (x, y) in a.iter_mut().zip(b) {
                *x += y;
            }
        }
        self
    }

    /// Total number of samples counted across all channels. Used to verify
    /// that pass 1 (accumulation) saw exactly as many samples as pass 2
    /// (synthesis) will emit, so a count mismatch fails loudly instead of
    /// silently zero-filling the deficit.
    pub fn total(&self) -> u64 {
        self.counts.iter().flat_map(|c| c.iter()).sum()
    }

    fn num_buckets(&self) -> usize {
        if self.bps == 8 {
            256
        } else {
            65536
        }
    }

    /// Map emission-order index to bucket index (ascending value order:
    /// signed data starts at the most negative value)
    fn bucket_at(&self, k: usize) -> usize {
        let buckets = self.num_buckets();
        if self.fmt == SAMPLEFORMAT_INT {
            (k + buckets / 2) & (buckets - 1)
        } else {
            k
        }
    }

    pub fn synthesizer(&self) -> Synthesizer<'_> {
        Synthesizer {
            hist: self,
            state: vec![(0usize, 0u64); self.counts.len()],
        }
    }
}

/// Streams the sorted sample sequence back out of a [`Histogram`],
/// row by row, without ever materializing the full plane.
pub struct Synthesizer<'a> {
    hist: &'a Histogram,
    /// Per channel: (emission-order position, samples already emitted from
    /// the current bucket)
    state: Vec<(usize, u64)>,
}

impl Synthesizer<'_> {
    fn next_value(&mut self, ch: usize) -> u16 {
        let buckets = self.hist.num_buckets();
        let (k, used) = &mut self.state[ch];
        while *k < buckets {
            let bucket = self.hist.bucket_at(*k);
            if *used < self.hist.counts[ch][bucket] {
                *used += 1;
                return bucket as u16;
            }
            *k += 1;
            *used = 0;
        }
        // Only reachable if more samples are requested than were counted
        0
    }

    /// Fill an interleaved output row with the next sorted values
    pub fn synthesize_row(&mut self, out: &mut [u8]) {
        let spp = self.hist.counts.len();
        if self.hist.bps == 8 {
            for (i, b) in out.iter_mut().enumerate() {
                *b = self.next_value(i % spp) as u8;
            }
        } else {
            for (i, s) in out.chunks_exact_mut(2).enumerate() {
                s.copy_from_slice(&self.next_value(i % spp).to_ne_bytes());
            }
        }
    }
}

// ============================================================================
// Parallel sort fallback (32/64-bit integers, floats, sub-byte)
// ============================================================================

/// Sort a macro-generated sample type in place inside a raw byte buffer.
macro_rules! sort_typed {
    ($buf:expr, $t:ty) => {{
        let size = std::mem::size_of::<$t>();
        let mut values: Vec<$t> = $buf
            .chunks_exact(size)
            .map(|c| <$t>::from_ne_bytes(c.try_into().unwrap()))
            .collect();
        values.par_sort_unstable();
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
        values.par_sort_unstable_by(|a, b| a.total_cmp(b));
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
        (8, SAMPLEFORMAT_INT) => sort_typed!(buf, i8),
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
        _ => buf.par_sort_unstable(),
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

    /// Simple deterministic pseudo-random byte generator (LCG)
    fn pseudo_random_bytes(n: usize, seed: u64) -> Vec<u8> {
        let mut state = seed;
        (0..n)
            .map(|_| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (state >> 33) as u8
            })
            .collect()
    }

    /// Histogram streaming must produce exactly the same bytes as the
    /// reference sort path, for any (spp, bps, fmt) it supports.
    fn assert_histogram_matches_sort(data: &[u8], spp: usize, bps: u16, fmt: u16) {
        // Reference: sort path
        let mut expected = data.to_vec();
        wipe_buffer(&mut expected, spp, bps, fmt);

        // Histogram path, accumulated in two unequal chunks to exercise merge
        let stride = spp * (bps as usize / 8);
        let split = (data.len() / stride / 3) * stride;
        let mut h1 = Histogram::new(spp, bps, fmt);
        h1.accumulate(&data[..split]);
        let mut h2 = Histogram::new(spp, bps, fmt);
        h2.accumulate(&data[split..]);
        let hist = h1.merge(h2);

        // Synthesize in several rows to exercise cursor continuation
        let mut synth = hist.synthesizer();
        let mut produced = vec![0u8; data.len()];
        let row_size = (data.len() / 4 / stride).max(1) * stride;
        for chunk in produced.chunks_mut(row_size) {
            synth.synthesize_row(chunk);
        }

        assert_eq!(produced, expected, "spp={} bps={} fmt={}", spp, bps, fmt);
    }

    #[test]
    fn histogram_matches_sort_u8() {
        let data = pseudo_random_bytes(4096, 1);
        assert_histogram_matches_sort(&data, 1, 8, SAMPLEFORMAT_UINT);
        assert_histogram_matches_sort(&data, 1, 8, SAMPLEFORMAT_INT);
    }

    #[test]
    fn histogram_matches_sort_u8_interleaved() {
        let data = pseudo_random_bytes(3 * 1024, 2);
        assert_histogram_matches_sort(&data, 3, 8, SAMPLEFORMAT_UINT);
    }

    #[test]
    fn histogram_matches_sort_u16() {
        let data = pseudo_random_bytes(8192, 3);
        assert_histogram_matches_sort(&data, 1, 16, SAMPLEFORMAT_UINT);
    }

    #[test]
    fn histogram_matches_sort_i16_signed_order() {
        // Include extreme signed values explicitly
        let mut data: Vec<u8> = [0i16, -1, 1, i16::MIN, i16::MAX, -32767, 32766, 100, -100, 0]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect();
        data.extend(pseudo_random_bytes(4096, 4));
        assert_histogram_matches_sort(&data, 1, 16, SAMPLEFORMAT_INT);
    }

    #[test]
    fn histogram_matches_sort_u16_interleaved() {
        let data = pseudo_random_bytes(2 * 3 * 512, 5);
        assert_histogram_matches_sort(&data, 3, 16, SAMPLEFORMAT_UINT);
    }

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
