//! Hyperslab reading and tile/strip decoding for GeoTIFF rasters.

use std::collections::HashMap;
use std::sync::Arc;

use async_tiff::decoder::DecoderRegistry;
use async_tiff::reader::AsyncFileReader;
use async_tiff::tags::PlanarConfiguration;
use async_tiff::{ImageFileDirectory, TypedArray};

use crate::data::blocks::BlockStoreError;
use crate::data::octant_block::OctantBlock;
use crate::data::slice_request::SliceRequest;

use super::coords::GeoSpatialBounds;

/// Fetch and decode an `OctantBlock` from an IFD for a given `SliceRequest`.
pub async fn fetch_geotiff_block(
    ifd: &ImageFileDirectory,
    request: &SliceRequest,
    reader: &dyn AsyncFileReader,
    decoder_registry: &DecoderRegistry,
) -> Result<OctantBlock, BlockStoreError> {
    let (band_opt, row_range, col_range) = parse_slice_request(request, ifd);
    let (row_start, row_end) = row_range;
    let (col_start, col_end) = col_range;

    let out_height = row_end.saturating_sub(row_start).max(1);
    let out_width = col_end.saturating_sub(col_start).max(1);

    let nodata_val = ifd.gdal_nodata().and_then(|s| s.trim().parse::<f64>().ok());

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
        read_band_region(ifd, reader, decoder_registry, &window, &mut buffer).await?;

        (
            Arc::from(buffer.into_boxed_slice()),
            vec![out_height, out_width],
            vec!["y".to_string(), "x".to_string()],
        )
    } else {
        // Multi-band 3D cube extraction
        let total_bands = ifd.samples_per_pixel() as usize;
        let band_count = total_bands;
        let mut buffer = vec![f32::NAN; band_count * out_height * out_width];

        for b in 0..band_count {
            let slice_start = b * out_height * out_width;
            let slice_end = slice_start + out_height * out_width;
            let window = ReadWindow {
                band: b,
                row_start,
                row_end,
                col_start,
                col_end,
                nodata_val,
            };
            read_band_region(
                ifd,
                reader,
                decoder_registry,
                &window,
                &mut buffer[slice_start..slice_end],
            )
            .await?;
        }

        (
            Arc::from(buffer.into_boxed_slice()),
            vec![band_count, out_height, out_width],
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

    Ok(OctantBlock::new(
        request.variable.clone(),
        out_shape,
        out_dims,
        origin,
        values,
        coords,
        HashMap::new(),
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
    } else if ifd.samples_per_pixel() == 1 {
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

/// Window parameters for hyperslab extraction.
#[derive(Debug, Clone, Copy)]
struct ReadWindow {
    band: usize,
    row_start: usize,
    row_end: usize,
    col_start: usize,
    col_end: usize,
    nodata_val: Option<f64>,
}

async fn read_band_region(
    ifd: &ImageFileDirectory,
    reader: &dyn AsyncFileReader,
    decoder_registry: &DecoderRegistry,
    window: &ReadWindow,
    out: &mut [f32],
) -> Result<(), BlockStoreError> {
    if ifd.tile_width().is_some() {
        read_tiled_region(ifd, reader, decoder_registry, window, out).await
    } else {
        read_striped_region(ifd, reader, decoder_registry, window, out).await
    }
}

async fn read_tiled_region(
    ifd: &ImageFileDirectory,
    reader: &dyn AsyncFileReader,
    decoder_registry: &DecoderRegistry,
    win: &ReadWindow,
    out: &mut [f32],
) -> Result<(), BlockStoreError> {
    let tw = ifd.tile_width().unwrap_or(ifd.image_width()) as usize;
    let th = ifd.tile_height().unwrap_or(ifd.image_height()) as usize;
    let out_w = win.col_end - win.col_start;

    let tile_x0 = win.col_start / tw;
    let tile_x1 = (win.col_end.saturating_sub(1)) / tw;
    let tile_y0 = win.row_start / th;
    let tile_y1 = (win.row_end.saturating_sub(1)) / th;

    for ty in tile_y0..=tile_y1 {
        for tx in tile_x0..=tile_x1 {
            let tile = ifd
                .fetch_tile(tx, ty, reader)
                .await
                .map_err(|e| format!("Failed to fetch tile ({tx}, {ty}): {e}"))?;

            let array = tile
                .decode(decoder_registry)
                .map_err(|e| format!("Failed to decode tile ({tx}, {ty}): {e}"))?;

            let (data, shape, _) = array.into_inner();
            let is_planar = ifd.planar_configuration() == PlanarConfiguration::Planar;

            let t_origin_x = tx * tw;
            let t_origin_y = ty * th;

            let r_min = win.row_start.max(t_origin_y);
            let r_max = win.row_end.min(t_origin_y + th);
            let c_min = win.col_start.max(t_origin_x);
            let c_max = win.col_end.min(t_origin_x + tw);

            for r in r_min..r_max {
                let local_r = r - t_origin_y;
                let dst_r = r - win.row_start;
                for c in c_min..c_max {
                    let local_c = c - t_origin_x;
                    let dst_c = c - win.col_start;
                    let val =
                        extract_f32_sample(&data, &shape, is_planar, win.band, local_r, local_c);
                    let final_val = if is_nodata(val, win.nodata_val) {
                        f32::NAN
                    } else {
                        val
                    };
                    out[dst_r * out_w + dst_c] = final_val;
                }
            }
        }
    }

    Ok(())
}

async fn read_striped_region(
    ifd: &ImageFileDirectory,
    reader: &dyn AsyncFileReader,
    decoder_registry: &DecoderRegistry,
    win: &ReadWindow,
    out: &mut [f32],
) -> Result<(), BlockStoreError> {
    let rps = ifd.rows_per_strip().unwrap_or(ifd.image_height()) as usize;
    let img_h = ifd.image_height() as usize;
    let out_w = win.col_end - win.col_start;

    let strip_offsets = ifd
        .strip_offsets()
        .ok_or_else(|| "TIFF has neither tiles nor strip offsets".to_string())?;

    let strip_0 = win.row_start / rps;
    let strip_1 = (win.row_end.saturating_sub(1)) / rps;

    for s_idx in strip_0..=strip_1 {
        if s_idx >= strip_offsets.len() {
            break;
        }

        let tile = ifd
            .fetch_strip(s_idx, reader)
            .await
            .map_err(|e| format!("Failed to read strip {s_idx}: {e}"))?;

        let array = tile
            .decode(decoder_registry)
            .map_err(|e| format!("Failed to decode strip {s_idx}: {e}"))?;

        let (data, shape, _) = array.into_inner();
        let is_planar = ifd.planar_configuration() == PlanarConfiguration::Planar;

        let strip_rows = rps.min(img_h.saturating_sub(s_idx * rps));
        let s_origin_y = s_idx * rps;
        let r_min = win.row_start.max(s_origin_y);
        let r_max = win.row_end.min(s_origin_y + strip_rows);

        for r in r_min..r_max {
            let local_r = r - s_origin_y;
            let dst_r = r - win.row_start;
            for c in win.col_start..win.col_end {
                let local_c = c;
                let dst_c = c - win.col_start;
                let val = extract_f32_sample(&data, &shape, is_planar, win.band, local_r, local_c);
                let final_val = if is_nodata(val, win.nodata_val) {
                    f32::NAN
                } else {
                    val
                };
                out[dst_r * out_w + dst_c] = final_val;
            }
        }
    }

    Ok(())
}

fn extract_f32_sample(
    data: &TypedArray,
    shape: &[usize; 3],
    is_planar: bool,
    band: usize,
    row: usize,
    col: usize,
) -> f32 {
    let idx = if is_planar {
        // [bands, height, width]
        let h = shape[1];
        let w = shape[2];
        band * h * w + row * w + col
    } else {
        // [height, width, bands]
        let w = shape[1];
        let b = shape[2];
        row * w * b + col * b + band
    };

    match data {
        TypedArray::UInt8(v) => v.get(idx).copied().map(|x| x as f32).unwrap_or(f32::NAN),
        TypedArray::UInt16(v) => v.get(idx).copied().map(|x| x as f32).unwrap_or(f32::NAN),
        TypedArray::UInt32(v) => v.get(idx).copied().map(|x| x as f32).unwrap_or(f32::NAN),
        TypedArray::UInt64(v) => v.get(idx).copied().map(|x| x as f32).unwrap_or(f32::NAN),
        TypedArray::Int8(v) => v.get(idx).copied().map(|x| x as f32).unwrap_or(f32::NAN),
        TypedArray::Int16(v) => v.get(idx).copied().map(|x| x as f32).unwrap_or(f32::NAN),
        TypedArray::Int32(v) => v.get(idx).copied().map(|x| x as f32).unwrap_or(f32::NAN),
        TypedArray::Int64(v) => v.get(idx).copied().map(|x| x as f32).unwrap_or(f32::NAN),
        TypedArray::Float32(v) => v.get(idx).copied().unwrap_or(f32::NAN),
        TypedArray::Float64(v) => v.get(idx).copied().map(|x| x as f32).unwrap_or(f32::NAN),
        TypedArray::Bool(v) => v
            .get(idx)
            .copied()
            .map(|b| if b { 1.0 } else { 0.0 })
            .unwrap_or(f32::NAN),
    }
}

fn is_nodata(val: f32, nodata: Option<f64>) -> bool {
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
