//! Hyperslab block slicing coordinator for TIFF/GeoTIFF datasets.

use std::collections::HashMap;
use std::sync::Arc;

use async_tiff::ImageFileDirectory;
use async_tiff::decoder::DecoderRegistry;
use async_tiff::reader::{AsyncFileReader, Endianness};
use async_tiff::tags::{PhotometricInterpretation, PlanarConfiguration, Predictor, SampleFormat};

use crate::data::blocks::BlockStoreError;
use crate::data::octant_block::OctantBlock;
use crate::data::slice_request::SliceRequest;

use super::blit::{ReadWindow, blit_chunk_to_window};
use super::coords::GeoSpatialBounds;
use super::decode::unpredict_buffer;
use super::palette::apply_colormap;

/// Fetch and decode an `OctantBlock` from an IFD for a given `SliceRequest`.
pub async fn fetch_geotiff_block(
    ifd: &ImageFileDirectory,
    endianness: Endianness,
    request: &SliceRequest,
    reader: &dyn AsyncFileReader,
    decoder_registry: &DecoderRegistry,
) -> Result<OctantBlock, BlockStoreError> {
    let (band_opt, (row_start, row_end), (col_start, col_end)) = parse_slice_request(request, ifd);
    let out_h = row_end.saturating_sub(row_start).max(1);
    let out_w = col_end.saturating_sub(col_start).max(1);
    let plane_len = out_h * out_w;
    let nodata_val = ifd.gdal_nodata().and_then(|s| s.trim().parse::<f64>().ok());
    let window = ReadWindow {
        row_start,
        row_end,
        col_start,
        col_end,
        nodata_val,
    };

    let is_palette = ifd.photometric_interpretation() == PhotometricInterpretation::RGBPalette;
    let colormap = ifd.colormap();

    let (values, out_shape, out_dims) = if let Some(band_idx) = band_opt {
        let mut buffer = vec![f32::NAN; plane_len];
        read_region(
            ifd,
            endianness,
            reader,
            decoder_registry,
            &window,
            &[band_idx],
            &mut [&mut buffer],
        )
        .await?;
        if is_palette
            && let Some(cmap) = colormap
            && let Some(ch) = palette_channel(&request.variable)
        {
            let bits = ifd.bits_per_sample().first().copied().unwrap_or(8);
            buffer = apply_colormap(&buffer, cmap, bits, ch);
        }
        (
            Arc::from(buffer.into_boxed_slice()),
            vec![out_h, out_w],
            vec!["y".into(), "x".into()],
        )
    } else {
        let total_bands = if is_palette && colormap.is_some() {
            3
        } else {
            ifd.samples_per_pixel() as usize
        };
        let mut buffer = vec![f32::NAN; total_bands * plane_len];
        if is_palette && let Some(cmap) = colormap {
            let mut idx_buf = vec![f32::NAN; plane_len];
            read_region(
                ifd,
                endianness,
                reader,
                decoder_registry,
                &window,
                &[0],
                &mut [&mut idx_buf],
            )
            .await?;
            let bits = ifd.bits_per_sample().first().copied().unwrap_or(8);
            for ch in 0..3 {
                let (start, end) = (ch * plane_len, (ch + 1) * plane_len);
                buffer[start..end].copy_from_slice(&apply_colormap(&idx_buf, cmap, bits, ch));
            }
        } else {
            let (mut band_slices, bands): (Vec<&mut [f32]>, Vec<usize>) = buffer
                .chunks_mut(plane_len)
                .enumerate()
                .take(total_bands)
                .map(|(i, chunk)| (chunk, i))
                .unzip();
            read_region(
                ifd,
                endianness,
                reader,
                decoder_registry,
                &window,
                &bands,
                &mut band_slices,
            )
            .await?;
        }
        (
            Arc::from(buffer.into_boxed_slice()),
            vec![total_bands, out_h, out_w],
            vec!["band".into(), "y".into(), "x".into()],
        )
    };

    let geo_bounds = GeoSpatialBounds::from_ifd(ifd);
    let mut coords = HashMap::new();
    coords.insert(
        "x".into(),
        geo_bounds.compute_x_coords(ifd.image_width() as usize, col_start, col_end),
    );
    coords.insert(
        "y".into(),
        geo_bounds.compute_y_coords(ifd.image_height() as usize, row_start, row_end),
    );

    let origin = if band_opt.is_some() {
        vec![row_start, col_start]
    } else {
        vec![0, row_start, col_start]
    };
    let mut attributes = HashMap::new();
    if ifd.photometric_interpretation() == PhotometricInterpretation::CMYK {
        attributes.insert("photometric".into(), "cmyk".into());
        attributes.insert("color_space".into(), "cmyk".into());
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

fn palette_channel(var: &str) -> Option<usize> {
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

async fn read_region(
    ifd: &ImageFileDirectory,
    endianness: Endianness,
    reader: &dyn AsyncFileReader,
    decoder_registry: &DecoderRegistry,
    win: &ReadWindow,
    target_bands: &[usize],
    out_slices: &mut [&mut [f32]],
) -> Result<(), BlockStoreError> {
    let (img_w, img_h) = (ifd.image_width() as usize, ifd.image_height() as usize);
    let is_planar = ifd.planar_configuration() == PlanarConfiguration::Planar;
    let samples = ifd.samples_per_pixel() as usize;
    let bits = ifd.bits_per_sample().first().copied().unwrap_or(8);
    let sample_fmt = ifd
        .sample_format()
        .first()
        .copied()
        .unwrap_or(SampleFormat::Uint);
    let predictor = ifd.predictor().unwrap_or(Predictor::None);
    let is_white_zero = ifd.photometric_interpretation() == PhotometricInterpretation::WhiteIsZero;

    let decoder = decoder_registry
        .as_ref()
        .get(&ifd.compression())
        .ok_or_else(|| format!("Unsupported compression: {:?}", ifd.compression()))?;

    let is_tiled = ifd.tile_width().is_some();
    let tw = ifd.tile_width().map(|v| v as usize).unwrap_or(img_w).max(1);
    let th = ifd
        .tile_height()
        .map(|v| v as usize)
        .unwrap_or_else(|| ifd.rows_per_strip().unwrap_or(img_h as u32) as usize)
        .min(img_h)
        .max(1);

    let (tiles_x, tiles_y) = (img_w.div_ceil(tw), img_h.div_ceil(th));
    let chunks_per_band = tiles_x * tiles_y;
    let (tx0, tx1) = (
        win.col_start / tw,
        ((win.col_end.saturating_sub(1)) / tw).min(tiles_x.saturating_sub(1)),
    );
    let (ty0, ty1) = (
        win.row_start / th,
        ((win.row_end.saturating_sub(1)) / th).min(tiles_y.saturating_sub(1)),
    );

    let offsets = ifd
        .tile_offsets()
        .or_else(|| ifd.strip_offsets())
        .ok_or("Missing chunk offsets")?;
    let byte_counts = ifd
        .tile_byte_counts()
        .or_else(|| ifd.strip_byte_counts())
        .ok_or("Missing chunk byte counts")?;

    for ty in ty0..=ty1 {
        let orig_y = ty * th;
        let buf_h = if is_tiled {
            th
        } else {
            th.min(img_h.saturating_sub(orig_y))
        };
        for tx in tx0..=tx1 {
            let (orig_x, buf_w) = (tx * tw, tw);
            if is_planar {
                for (i, &band) in target_bands.iter().enumerate() {
                    let idx = band * chunks_per_band + ty * tiles_x + tx;
                    if idx >= offsets.len() {
                        continue;
                    }
                    let raw = reader
                        .get_bytes(offsets[idx]..offsets[idx] + byte_counts[idx])
                        .await
                        .map_err(|e| e.to_string())?;
                    let decomp = decoder
                        .decode_tile(
                            raw,
                            ifd.photometric_interpretation(),
                            ifd.jpeg_tables(),
                            1,
                            bits,
                            None,
                        )
                        .map_err(|e| e.to_string())?;
                    let unpred = unpredict_buffer(decomp, predictor, 1, bits, buf_w, endianness)
                        .map_err(|e| e.to_string())?;
                    if let Some(out_slice) = out_slices.get_mut(i) {
                        blit_chunk_to_window(
                            &unpred,
                            buf_w,
                            buf_h,
                            1,
                            true,
                            orig_x,
                            orig_y,
                            sample_fmt,
                            bits,
                            is_white_zero,
                            win,
                            &[0],
                            &mut [out_slice],
                        );
                    }
                }
            } else {
                let idx = ty * tiles_x + tx;
                if idx >= offsets.len() {
                    continue;
                }
                let raw = reader
                    .get_bytes(offsets[idx]..offsets[idx] + byte_counts[idx])
                    .await
                    .map_err(|e| e.to_string())?;
                let decomp = decoder
                    .decode_tile(
                        raw,
                        ifd.photometric_interpretation(),
                        ifd.jpeg_tables(),
                        samples as u16,
                        bits,
                        None,
                    )
                    .map_err(|e| e.to_string())?;
                let unpred = unpredict_buffer(decomp, predictor, samples, bits, buf_w, endianness)
                    .map_err(|e| e.to_string())?;
                blit_chunk_to_window(
                    &unpred,
                    buf_w,
                    buf_h,
                    samples,
                    false,
                    orig_x,
                    orig_y,
                    sample_fmt,
                    bits,
                    is_white_zero,
                    win,
                    target_bands,
                    out_slices,
                );
            }
        }
    }
    Ok(())
}

fn parse_slice_request(
    request: &SliceRequest,
    ifd: &ImageFileDirectory,
) -> (Option<usize>, (usize, usize), (usize, usize)) {
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
    (band_idx, (row_start, row_end), (col_start, col_end))
}
