//! Buffer copying routines for strided and contiguous hyperslab slices.

/// Copies values for contiguous row and column slices.
pub fn copy_contiguous_slice(
    values: &[f32],
    base_offset: usize,
    y_start: usize,
    y_end: usize,
    stride_y: usize,
    width: usize,
) -> Vec<f32> {
    let slice_len = width * (y_end - y_start);
    let slice_start = base_offset + y_start * stride_y;
    let slice_end = slice_start + slice_len;

    if slice_end <= values.len() {
        values[slice_start..slice_end].to_vec()
    } else {
        let mut result = Vec::with_capacity(slice_len);
        for y in y_start..y_end {
            let row_start = base_offset + y * stride_y;
            let row_end = row_start + width;
            if row_end <= values.len() {
                result.extend_from_slice(&values[row_start..row_end]);
            } else {
                for x in 0..width {
                    result.push(values.get(row_start + x).copied().unwrap_or(f32::NAN));
                }
            }
        }
        result
    }
}

/// Copies values when rows are contiguous in X (stride_x == 1).
pub fn copy_row_contiguous_slice(
    values: &[f32],
    base_offset: usize,
    y_start: usize,
    y_end: usize,
    stride_y: usize,
    x_start: usize,
    width: usize,
) -> Vec<f32> {
    let slice_len = width * (y_end - y_start);
    let mut result = Vec::with_capacity(slice_len);

    for y in y_start..y_end {
        let row_start = base_offset + y * stride_y + x_start;
        let row_end = row_start + width;
        if row_end <= values.len() {
            result.extend_from_slice(&values[row_start..row_end]);
        } else {
            for x in 0..width {
                result.push(values.get(row_start + x).copied().unwrap_or(f32::NAN));
            }
        }
    }

    result
}

/// Copies values for arbitrary strided X and Y slices.
#[allow(clippy::too_many_arguments)]
pub fn copy_strided_slice(
    values: &[f32],
    base_offset: usize,
    y_start: usize,
    y_end: usize,
    stride_y: usize,
    x_start: usize,
    x_end: usize,
    stride_x: usize,
) -> Vec<f32> {
    let width = x_end.saturating_sub(x_start);
    let height = y_end.saturating_sub(y_start);
    let mut result = Vec::with_capacity(width * height);

    for y in y_start..y_end {
        let row_start = base_offset + y * stride_y;
        for x in x_start..x_end {
            let idx = row_start + x * stride_x;
            result.push(values.get(idx).copied().unwrap_or(f32::NAN));
        }
    }

    result
}
