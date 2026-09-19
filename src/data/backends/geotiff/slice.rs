//! Hyperslab reading coordinator for GeoTIFF rasters.

use std::collections::HashMap;
use std::sync::Arc;

use async_tiff::ImageFileDirectory;
use async_tiff::decoder::DecoderRegistry;
use async_tiff::reader::AsyncFileReader;
use async_tiff::tags::PhotometricInterpretation;

use crate::data::blocks::BlockStoreError;
use crate::data::octant_block::OctantBlock;
use crate::data::slice_request::SliceRequest;

use super::coords::GeoSpatialBounds;
use super::palette::apply_colormap;
use super::slice_striped::read_striped_region;
use super::slice_tiled::read_tiled_region;

/// Fetch and decode an `OctantBlock` from an IFD for a given `SliceRequest`.
pub async fn fetch_geotiff_block(
    ifd: &ImageFileDirectory,
    request: &SliceRequest,
    reader: &dyn AsyncFileReader,
    _decoder_registry: &DecoderRegistry,
) -> Result<OctantBlock, BlockStoreError> {
    let (band_opt, row_range, col_range) = parse_slice_request(request, ifd);
    let (row_start, row_end) = row_range;
    let (col_start, col_end) = col_range;

    let out_height = row_end.saturating_sub(row_start).max(1);
    let out_width = col_end.saturating_sub(col_start).max(1);
    let nodata_val = ifd.gdal_nodata().and_then(|s| s.trim().parse::<f64>().ok());

    let is_palette = ifd.photometric_interpretation() == PhotometricInterpretation::RGBPalette;
    let colormap = ifd.colormap();

    let (values, out_shape, out_dims) = if let Some(band_idx) = band_opt {
        // Single 2D band extraction
        let mut buffer = vec![f32::NAN; out_height * out_width];
        let window = ReadWindow {
            band: band_idx,
            row_start,
            row_end,
            col_start,
            col_end,
            nodata_val,
        };
        read_band_region(ifd, reader, &window, &mut buffer).await?;

        if is_palette
            && let Some(cmap) = colormap
            && (request.variable.starts_with("red")
                || request.variable.starts_with("green")
                || request.variable.starts_with("blue"))
        {
            let ch = if request.variable.starts_with("red") {
                0
            } else if request.variable.starts_with("green") {
                1
            } else {
                2
            };
            let bits = ifd.bits_per_sample().first().copied().unwrap_or(8);
            buffer = apply_colormap(&buffer, cmap, bits, ch);
        }

        (
            Arc::from(buffer.into_boxed_slice()),
            vec![out_height, out_width],
            vec!["y".to_string(), "x".to_string()],
        )
    } else {
        // Multi-band 3D cube extraction
        let total_bands = if is_palette && colormap.is_some() {
            3
        } else {
            ifd.samples_per_pixel() as usize
        };
        let mut buffer = vec![f32::NAN; total_bands * out_height * out_width];

        for b in 0..total_bands {
            let slice_start = b * out_height * out_width;
            let slice_end = slice_start + out_height * out_width;
            let window = ReadWindow {
                band: if is_palette { 0 } else { b },
                row_start,
                row_end,
                col_start,
                col_end,
                nodata_val,
            };
            read_band_region(ifd, reader, &window, &mut buffer[slice_start..slice_end]).await?;

            if is_palette && let Some(cmap) = colormap {
                let bits = ifd.bits_per_sample().first().copied().unwrap_or(8);
                let mapped = apply_colormap(&buffer[slice_start..slice_end], cmap, bits, b);
                buffer[slice_start..slice_end].copy_from_slice(&mapped);
            }
        }

        (
            Arc::from(buffer.into_boxed_slice()),
            vec![total_bands, out_height, out_width],
            vec!["band".to_string(), "y".to_string(), "x".to_string()],
        )
    };

    let geo_bounds = GeoSpatialBounds::from_ifd(ifd);
    let mut coords = HashMap::new();
    coords.insert(
        "x".to_string(),
        geo_bounds.compute_x_coords(ifd.image_width() as usize, col_start, col_end),
    );
    coords.insert(
        "y".to_string(),
        geo_bounds.compute_y_coords(ifd.image_height() as usize, row_start, row_end),
    );

    let origin = if band_opt.is_some() {
        vec![row_start, col_start]
    } else {
        vec![0, row_start, col_start]
    };

    let mut attributes = HashMap::new();
    if ifd.photometric_interpretation() == PhotometricInterpretation::CMYK {
        attributes.insert("photometric".to_string(), "cmyk".to_string());
        attributes.insert("color_space".to_string(), "cmyk".to_string());
    }

    Ok(OctantBlock::new(
        request.variable.clone(),
        out_shape,
        out_dims,
        origin,
        values,
        coords,
        attributes,
    ))
}

fn parse_slice_request(
    request: &SliceRequest,
    ifd: &ImageFileDirectory,
) -> (Option<usize>, (usize, usize), (usize, usize)) {
    let img_w = ifd.image_width() as usize;
    let img_h = ifd.image_height() as usize;
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

    (band_idx, (row_start, row_end), (col_start, col_end))
}

#[derive(Debug, Clone, Copy)]
pub struct ReadWindow {
    pub band: usize,
    pub row_start: usize,
    pub row_end: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub nodata_val: Option<f64>,
}

async fn read_band_region(
    ifd: &ImageFileDirectory,
    reader: &dyn AsyncFileReader,
    win: &ReadWindow,
    out: &mut [f32],
) -> Result<(), BlockStoreError> {
    if ifd.tile_width().is_some() {
        read_tiled_region(ifd, reader, win, out).await
    } else {
        read_striped_region(ifd, reader, win, out).await
    }
}

pub fn is_nodata(val: f32, nodata: Option<f64>) -> bool {
    if val.is_nan() {
        return true;
    }
    if let Some(nd) = nodata {
        let nd_f32 = nd as f32;
        if nd_f32.is_nan() {
            val.is_nan()
        } else {
            (val - nd_f32).abs() < 1e-6
        }
    } else {
        false
    }
}
