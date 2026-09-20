//! SliceRequest parsing and hyperslab window calculations for TIFF datasets.

use async_tiff::ImageFileDirectory;
use async_tiff::tags::PhotometricInterpretation;

use crate::data::slice_request::SliceRequest;

/// Parsed slice request details for TIFF extraction.
pub struct ParsedSlice {
    pub band_idx: Option<usize>,
    pub row_start: usize,
    pub row_end: usize,
    pub col_start: usize,
    pub col_end: usize,
}

/// Parse a `SliceRequest` against an `ImageFileDirectory`.
pub fn parse_slice_request(request: &SliceRequest, ifd: &ImageFileDirectory) -> ParsedSlice {
    let (img_w, img_h) = (ifd.image_width() as usize, ifd.image_height() as usize);
    let var = request.variable.trim();
    let band_idx = if var.contains("band_") {
        var.rsplit("band_")
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .map(|idx| idx.saturating_sub(1))
    } else if (ifd.samples_per_pixel() == 1
        && ifd.photometric_interpretation() != PhotometricInterpretation::RGBPalette)
        || !var.contains("raster")
    {
        Some(0)
    } else {
        None
    };

    let (row_sel, col_sel) = if band_idx.is_some() || request.selections.len() <= 2 {
        let r = request
            .selections
            .first()
            .map(|s| s.bounds())
            .unwrap_or((0, img_h));
        let c = request
            .selections
            .get(1)
            .map(|s| s.bounds())
            .unwrap_or((0, img_w));
        (r, c)
    } else {
        let r = request
            .selections
            .get(1)
            .map(|s| s.bounds())
            .unwrap_or((0, img_h));
        let c = request
            .selections
            .get(2)
            .map(|s| s.bounds())
            .unwrap_or((0, img_w));
        (r, c)
    };

    let row_start = row_sel.0.min(img_h);
    let row_end = row_sel.1.max(row_start + 1).min(img_h);
    let col_start = col_sel.0.min(img_w);
    let col_end = col_sel.1.max(col_start + 1).min(img_w);

    ParsedSlice {
        band_idx,
        row_start,
        row_end,
        col_start,
        col_end,
    }
}

/// Identify the RGB channel index (0 = Red, 1 = Green, 2 = Blue) from a variable name.
pub fn palette_channel(var: &str) -> Option<usize> {
    if var.starts_with("red") {
        Some(0)
    } else if var.starts_with("green") {
        Some(1)
    } else if var.starts_with("blue") {
        Some(2)
    } else {
        None
    }
}
