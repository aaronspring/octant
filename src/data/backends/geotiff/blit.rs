//! Direct sample blitting from decoded TIFF chunk buffers to output slices.

use async_tiff::tags::SampleFormat;

/// Sliced hyperslab window coordinate ranges.
#[derive(Debug, Clone, Copy)]
pub struct ReadWindow {
    pub row_start: usize,
    pub row_end: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub nodata_val: Option<f64>,
}

/// Blit decoded raw bytes from a chunk (tile or strip) directly into destination hyperslab slices.
#[allow(clippy::too_many_arguments)]
pub fn blit_chunk_to_window(
    raw: &[u8],
    chunk_w: usize,
    chunk_h: usize,
    samples_per_pixel: usize,
    is_planar: bool,
    origin_x: usize,
    origin_y: usize,
    sample_fmt: SampleFormat,
    bits_per_sample: u16,
    is_white_zero: bool,
    win: &ReadWindow,
    target_bands: &[usize],
    out_slices: &mut [&mut [f32]],
) {
    let out_w = win.col_end.saturating_sub(win.col_start).max(1);
    let r_min = win
        .row_start
        .max(origin_y)
        .min(win.row_end.min(origin_y + chunk_h));
    let r_max = win.row_end.min(origin_y + chunk_h);
    let c_min = win
        .col_start
        .max(origin_x)
        .min(win.col_end.min(origin_x + chunk_w));
    let c_max = win.col_end.min(origin_x + chunk_w);

    if r_min >= r_max || c_min >= c_max {
        return;
    }

    let nodata = win.nodata_val;
    let rows = r_min..r_max;
    let cols = c_min..c_max;

    match (sample_fmt, bits_per_sample) {
        (SampleFormat::Uint, 8) => {
            blit_typed(
                raw,
                chunk_w,
                chunk_h,
                samples_per_pixel,
                is_planar,
                origin_x,
                origin_y,
                win,
                out_w,
                rows,
                cols,
                target_bands,
                out_slices,
                nodata,
                |raw, idx, _, _, _| {
                    let b = raw.get(idx).copied().unwrap_or(0);
                    if is_white_zero {
                        (255 - b) as f32
                    } else {
                        b as f32
                    }
                },
            );
        }
        (SampleFormat::Int, 8) => {
            blit_typed(
                raw,
                chunk_w,
                chunk_h,
                samples_per_pixel,
                is_planar,
                origin_x,
                origin_y,
                win,
                out_w,
                rows,
                cols,
                target_bands,
                out_slices,
                nodata,
                |raw, idx, _, _, _| raw.get(idx).map(|&b| b as i8 as f32).unwrap_or(f32::NAN),
            );
        }
        (SampleFormat::Uint, 16) => {
            blit_typed(
                raw,
                chunk_w,
                chunk_h,
                samples_per_pixel,
                is_planar,
                origin_x,
                origin_y,
                win,
                out_w,
                rows,
                cols,
                target_bands,
                out_slices,
                nodata,
                |raw, idx, _, _, _| {
                    let off = idx * 2;
                    if off + 1 < raw.len() {
                        u16::from_ne_bytes([raw[off], raw[off + 1]]) as f32
                    } else {
                        f32::NAN
                    }
                },
            );
        }
        (SampleFormat::Int, 16) => {
            blit_typed(
                raw,
                chunk_w,
                chunk_h,
                samples_per_pixel,
                is_planar,
                origin_x,
                origin_y,
                win,
                out_w,
                rows,
                cols,
                target_bands,
                out_slices,
                nodata,
                |raw, idx, _, _, _| {
                    let off = idx * 2;
                    if off + 1 < raw.len() {
                        i16::from_ne_bytes([raw[off], raw[off + 1]]) as f32
                    } else {
                        f32::NAN
                    }
                },
            );
        }
        (SampleFormat::Float, 32) => {
            blit_typed(
                raw,
                chunk_w,
                chunk_h,
                samples_per_pixel,
                is_planar,
                origin_x,
                origin_y,
                win,
                out_w,
                rows,
                cols,
                target_bands,
                out_slices,
                nodata,
                |raw, idx, _, _, _| {
                    let off = idx * 4;
                    if off + 3 < raw.len() {
                        f32::from_ne_bytes([raw[off], raw[off + 1], raw[off + 2], raw[off + 3]])
                    } else {
                        f32::NAN
                    }
                },
            );
        }
        (SampleFormat::Float, 64) => {
            blit_typed(
                raw,
                chunk_w,
                chunk_h,
                samples_per_pixel,
                is_planar,
                origin_x,
                origin_y,
                win,
                out_w,
                rows,
                cols,
                target_bands,
                out_slices,
                nodata,
                |raw, idx, _, _, _| {
                    let off = idx * 8;
                    if off + 7 < raw.len() {
                        let b: [u8; 8] = [
                            raw[off],
                            raw[off + 1],
                            raw[off + 2],
                            raw[off + 3],
                            raw[off + 4],
                            raw[off + 5],
                            raw[off + 6],
                            raw[off + 7],
                        ];
                        f64::from_ne_bytes(b) as f32
                    } else {
                        f32::NAN
                    }
                },
            );
        }
        _ => {
            blit_typed(
                raw,
                chunk_w,
                chunk_h,
                samples_per_pixel,
                is_planar,
                origin_x,
                origin_y,
                win,
                out_w,
                rows,
                cols,
                target_bands,
                out_slices,
                nodata,
                |raw, idx, r, c, band| {
                    get_sample_generic(
                        raw,
                        chunk_w,
                        samples_per_pixel,
                        is_planar,
                        idx,
                        r,
                        c,
                        band,
                        sample_fmt,
                        bits_per_sample,
                        is_white_zero,
                    )
                },
            );
        }
    }
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn blit_typed<F>(
    raw: &[u8],
    w: usize,
    h: usize,
    samples: usize,
    is_planar: bool,
    orig_x: usize,
    orig_y: usize,
    win: &ReadWindow,
    out_w: usize,
    rows: std::ops::Range<usize>,
    cols: std::ops::Range<usize>,
    target_bands: &[usize],
    out_slices: &mut [&mut [f32]],
    nodata: Option<f64>,
    extract: F,
) where
    F: Fn(&[u8], usize, usize, usize, usize) -> f32,
{
    let plane_size = w * h;
    for r in rows {
        let local_r = r - orig_y;
        let dst_r = r - win.row_start;
        for c in cols.clone() {
            let local_c = c - orig_x;
            let dst_c = c - win.col_start;
            let dst_idx = dst_r * out_w + dst_c;

            for (i, &band) in target_bands.iter().enumerate() {
                if let Some(out_slice) = out_slices.get_mut(i) {
                    let linear_idx = if is_planar {
                        band * plane_size + local_r * w + local_c
                    } else {
                        local_r * (w * samples) + local_c * samples + band
                    };
                    let val = extract(raw, linear_idx, local_r, local_c, band);
                    out_slice[dst_idx] = if is_nodata(val, nodata) {
                        f32::NAN
                    } else {
                        val
                    };
                }
            }
        }
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn get_sample_generic(
    raw: &[u8],
    w: usize,
    samples: usize,
    is_planar: bool,
    linear_idx: usize,
    r: usize,
    c: usize,
    band: usize,
    fmt: SampleFormat,
    bits: u16,
    is_white_zero: bool,
) -> f32 {
    match (fmt, bits) {
        (SampleFormat::Uint, 1) => {
            let row_bytes = (w * if is_planar { 1 } else { samples }).div_ceil(8);
            let s_in_row = if is_planar { c } else { c * samples + band };
            let byte_idx = r * row_bytes + s_in_row / 8;
            let bit_idx = 7 - (s_in_row % 8);
            let bit = raw.get(byte_idx).map(|&b| (b >> bit_idx) & 1).unwrap_or(0);
            if is_white_zero {
                if bit == 0 { 1.0 } else { 0.0 }
            } else {
                bit as f32
            }
        }
        (SampleFormat::Uint, 4) => {
            let row_bytes = (w * if is_planar { 1 } else { samples }).div_ceil(2);
            let s_in_row = if is_planar { c } else { c * samples + band };
            let byte_idx = r * row_bytes + s_in_row / 2;
            let byte = raw.get(byte_idx).copied().unwrap_or(0);
            let nibble = if s_in_row % 2 == 0 {
                byte >> 4
            } else {
                byte & 0x0F
            };
            nibble as f32
        }
        (SampleFormat::Uint, 12) => {
            let row_samples = w * if is_planar { 1 } else { samples };
            let row_bytes = (row_samples * 12).div_ceil(8);
            let s_in_row = if is_planar { c } else { c * samples + band };
            let pair_idx = s_in_row / 2;
            let byte_idx = r * row_bytes + pair_idx * 3;
            if s_in_row % 2 == 0 {
                let b0 = raw.get(byte_idx).copied().unwrap_or(0) as u16;
                let b1 = raw.get(byte_idx + 1).copied().unwrap_or(0) as u16;
                ((b0 << 4) | (b1 >> 4)) as f32
            } else {
                let b1 = raw.get(byte_idx + 1).copied().unwrap_or(0) as u16;
                let b2 = raw.get(byte_idx + 2).copied().unwrap_or(0) as u16;
                (((b1 & 0x0F) << 8) | b2) as f32
            }
        }
        (SampleFormat::Float, 16) => {
            let off = linear_idx * 2;
            if off + 1 < raw.len() {
                half::f16::from_bits(u16::from_ne_bytes([raw[off], raw[off + 1]])).to_f32()
            } else {
                f32::NAN
            }
        }
        (SampleFormat::Uint, 32) => {
            let off = linear_idx * 4;
            raw.get(off..off + 4)
                .and_then(|s| s.try_into().ok())
                .map(|b| u32::from_ne_bytes(b) as f32)
                .unwrap_or(f32::NAN)
        }
        (SampleFormat::Int, 32) => {
            let off = linear_idx * 4;
            raw.get(off..off + 4)
                .and_then(|s| s.try_into().ok())
                .map(|b| i32::from_ne_bytes(b) as f32)
                .unwrap_or(f32::NAN)
        }
        (SampleFormat::Uint, 64) => {
            let off = linear_idx * 8;
            raw.get(off..off + 8)
                .and_then(|s| s.try_into().ok())
                .map(|b| u64::from_ne_bytes(b) as f32)
                .unwrap_or(f32::NAN)
        }
        (SampleFormat::Int, 64) => {
            let off = linear_idx * 8;
            raw.get(off..off + 8)
                .and_then(|s| s.try_into().ok())
                .map(|b| i64::from_ne_bytes(b) as f32)
                .unwrap_or(f32::NAN)
        }
        _ => raw.get(linear_idx).map(|&b| b as f32).unwrap_or(f32::NAN),
    }
}

/// Check if sample equals the specified nodata sentinel value.
#[inline]
pub fn is_nodata(val: f32, nodata: Option<f64>) -> bool {
    if val.is_nan() {
        return true;
    }
    if let Some(nd) = nodata {
        let nd_f32 = nd as f32;
        if nd_f32.is_nan() {
            val.is_nan()
        } else if val.to_bits() == nd_f32.to_bits() {
            true
        } else {
            let diff = (val - nd_f32).abs();
            let threshold = (nd_f32.abs() * 1e-5).max(1e-6);
            diff <= threshold
        }
    } else {
        false
    }
}
