//! RGB and CMYK composite slicing to 24-bit TrueColor `MatrixData`.

use crate::data::matrix_data::MatrixData;
use crate::data::octant_block::OctantBlock;

/// Slice an `OctantBlock` into a 24-bit TrueColor composite `MatrixData`.
pub fn slice_rgb_composite(
    block: &OctantBlock,
    channels: [usize; 3],
    anim_extent: usize,
) -> Option<MatrixData> {
    if block.shape.len() < 3 || block.shape[0] < 3 {
        return None;
    }
    let (num_bands, height, width) = (block.shape[0], block.shape[1], block.shape[2]);
    let plane_size = height.checked_mul(width)?;

    let is_cmyk = num_bands >= 4
        && block
            .attributes
            .get("photometric")
            .or_else(|| block.attributes.get("color_space"))
            .is_some_and(|s| s.eq_ignore_ascii_case("cmyk"));

    if is_cmyk {
        let k_end = 4 * plane_size;
        if k_end > block.values.len() {
            return None;
        }
        let (c, m, y, k) = (
            &block.values[0..plane_size],
            &block.values[plane_size..2 * plane_size],
            &block.values[2 * plane_size..3 * plane_size],
            &block.values[3 * plane_size..4 * plane_size],
        );
        let max_val = [c, m, y, k]
            .iter()
            .map(|s| crate::utils::compute_finite_min_max(s).1)
            .fold(0.0f32, f32::max);
        let scale = if max_val > 255.0 {
            1.0 / max_val
        } else if max_val > 1.0 {
            1.0 / 255.0
        } else {
            1.0
        };

        let mut values = Vec::with_capacity(plane_size);
        for i in 0..plane_size {
            if c[i].is_nan() || m[i].is_nan() || y[i].is_nan() || k[i].is_nan() {
                values.push(f32::NAN);
            } else {
                let (c_n, m_n, y_n, k_n) = (
                    (c[i] * scale).clamp(0.0, 1.0),
                    (m[i] * scale).clamp(0.0, 1.0),
                    (y[i] * scale).clamp(0.0, 1.0),
                    (k[i] * scale).clamp(0.0, 1.0),
                );
                let r = ((1.0 - c_n) * (1.0 - k_n) * 255.0).clamp(0.0, 255.0);
                let g = ((1.0 - m_n) * (1.0 - k_n) * 255.0).clamp(0.0, 255.0);
                let b = ((1.0 - y_n) * (1.0 - k_n) * 255.0).clamp(0.0, 255.0);
                values.push(pack_rgb(r, g, b));
            }
        }
        return Some(MatrixData::new(
            width,
            height,
            values,
            0.0,
            16777215.0,
            format!("{} (CMYK Composite)", block.variable_name),
            anim_extent,
        ));
    }

    let [r_ch, g_ch, b_ch] = channels.map(|c| c.min(num_bands.saturating_sub(1)));
    let (r_off, g_off, b_off) = (r_ch * plane_size, g_ch * plane_size, b_ch * plane_size);
    if r_off + plane_size > block.values.len()
        || g_off + plane_size > block.values.len()
        || b_off + plane_size > block.values.len()
    {
        return None;
    }

    let (r, g, b) = (
        &block.values[r_off..r_off + plane_size],
        &block.values[g_off..g_off + plane_size],
        &block.values[b_off..b_off + plane_size],
    );
    let (r_min, r_max) = crate::utils::compute_finite_min_max(r);
    let (g_min, g_max) = crate::utils::compute_finite_min_max(g);
    let (b_min, b_max) = crate::utils::compute_finite_min_max(b);
    let (g_min, g_max) = (r_min.min(g_min).min(b_min), r_max.max(g_max).max(b_max));

    let (scale, offset) = if g_min >= 0.0 && g_max <= 255.0 {
        (1.0, 0.0)
    } else if g_min >= 0.0 && g_max <= 1.0 {
        (255.0, 0.0)
    } else if g_min >= 0.0 && g_max > 255.0 {
        (255.0 / g_max, 0.0)
    } else if g_max > g_min {
        (255.0 / (g_max - g_min), g_min)
    } else {
        (1.0, 0.0)
    };

    let mut values = Vec::with_capacity(plane_size);
    for i in 0..plane_size {
        if r[i].is_nan() || g[i].is_nan() || b[i].is_nan() {
            values.push(f32::NAN);
        } else {
            let r_n = ((r[i] - offset) * scale).clamp(0.0, 255.0);
            let g_n = ((g[i] - offset) * scale).clamp(0.0, 255.0);
            let b_n = ((b[i] - offset) * scale).clamp(0.0, 255.0);
            values.push(pack_rgb(r_n, g_n, b_n));
        }
    }

    Some(MatrixData::new(
        width,
        height,
        values,
        0.0,
        16777215.0,
        format!("{} (RGB Composite)", block.variable_name),
        anim_extent,
    ))
}

#[inline(always)]
fn pack_rgb(r: f32, g: f32, b: f32) -> f32 {
    let packed = (r as u32) | ((g as u32) << 8) | ((b as u32) << 16);
    packed as f32
}
