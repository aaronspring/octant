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
    let nodata_val = ifd.gdal_nodata().and_then(|s| s.trim().parse::<f64>().ok());

    let is_palette = ifd.photometric_interpretation() == PhotometricInterpretation::RGBPalette;
    let colormap = ifd.colormap();

    let (values, out_shape, out_dims) = if let Some(band_idx) = band_opt {
        let mut buffer = vec![f32::NAN; out_h * out_w];
        let window = ReadWindow {
            row_start,
            row_end,
            col_start,
            col_end,
            nodata_val,
        };
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
            vec![out_h, out_w],
            vec!["y".to_string(), "x".to_string()],
        )
    } else {
        let total_bands = if is_palette && colormap.is_some() {
            3
        } else {
            ifd.samples_per_pixel() as usize
        };
        let mut buffer = vec![f32::NAN; total_bands * out_h * out_w];
        let plane_len = out_h * out_w;

        let window = ReadWindow {
            row_start,
            row_end,
            col_start,
            col_end,
            nodata_val,
        };

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
            for b in 0..3 {
                let slice = &mut buffer[b * plane_len..(b + 1) * plane_len];
                slice.copy_from_slice(&apply_colormap(&idx_buf, cmap, bits, b));
            }
        } else {
            let target_bands: Vec<usize> = (0..total_bands).collect();
            let mut slices: Vec<&mut [f32]> = buffer.chunks_exact_mut(plane_len).collect();
            read_region(
                ifd,
                endianness,
                reader,
                decoder_registry,
                &window,
                &target_bands,
                &mut slices,
            )
            .await?;
        }

        (
            Arc::from(buffer.into_boxed_slice()),
            vec![total_bands, out_h, out_w],
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

async fn read_region(
    ifd: &ImageFileDirectory,
    endianness: Endianness,
    reader: &dyn AsyncFileReader,
    decoder_registry: &DecoderRegistry,
    win: &ReadWindow,
    target_bands: &[usize],
    out_slices: &mut [&mut [f32]],
) -> Result<(), BlockStoreError> {
    let img_w = ifd.image_width() as usize;
    let img_h = ifd.image_height() as usize;
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

    if let (Some(tw), Some(th)) = (ifd.tile_width(), ifd.tile_height()) {
        let (tw, th) = (tw as usize, th as usize);
        let (tiles_x, tiles_y) = ifd.tile_count().unwrap_or((1, 1));
        let tx0 = win.col_start / tw;
        let tx1 = ((win.col_end.saturating_sub(1)) / tw).min(tiles_x.saturating_sub(1));
        let ty0 = win.row_start / th;
        let ty1 = ((win.row_end.saturating_sub(1)) / th).min(tiles_y.saturating_sub(1));

        let tile_offsets = ifd.tile_offsets().ok_or("Missing tile offsets")?;
        let tile_byte_counts = ifd.tile_byte_counts().ok_or("Missing tile byte counts")?;
        let tiles_per_band = tiles_x * tiles_y;

        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let orig_x = tx * tw;
                let orig_y = ty * th;

                if is_planar {
                    for (i, &band) in target_bands.iter().enumerate() {
                        let idx = band * tiles_per_band + ty * tiles_x + tx;
                        if idx >= tile_offsets.len() {
                            continue;
                        }
                        let raw = reader
                            .get_bytes(tile_offsets[idx]..tile_offsets[idx] + tile_byte_counts[idx])
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
                        let unpred = unpredict_buffer(decomp, predictor, 1, bits, tw, endianness)
                            .map_err(|e| e.to_string())?;
                        if let Some(out_slice) = out_slices.get_mut(i) {
                            blit_chunk_to_window(
                                &unpred,
                                tw,
                                th,
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
                    if idx >= tile_offsets.len() {
                        continue;
                    }
                    let raw = reader
                        .get_bytes(tile_offsets[idx]..tile_offsets[idx] + tile_byte_counts[idx])
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
                    let unpred = unpredict_buffer(decomp, predictor, samples, bits, tw, endianness)
                        .map_err(|e| e.to_string())?;
                    blit_chunk_to_window(
                        &unpred,
                        tw,
                        th,
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
    } else {
        let rps = (ifd.rows_per_strip().unwrap_or(img_h as u32) as usize).min(img_h);
        let strips_per_band = img_h.div_ceil(rps.max(1));
        let s0 = win.row_start / rps.max(1);
        let s1 =
            ((win.row_end.saturating_sub(1)) / rps.max(1)).min(strips_per_band.saturating_sub(1));

        let strip_offsets = ifd.strip_offsets().ok_or("Missing strip offsets")?;
        let strip_byte_counts = ifd.strip_byte_counts().ok_or("Missing strip byte counts")?;

        for s_idx in s0..=s1 {
            let orig_y = s_idx * rps;
            let strip_h = rps.min(img_h.saturating_sub(orig_y));

            if is_planar {
                for (i, &band) in target_bands.iter().enumerate() {
                    let idx = band * strips_per_band + s_idx;
                    if idx >= strip_offsets.len() {
                        continue;
                    }
                    let raw = reader
                        .get_bytes(strip_offsets[idx]..strip_offsets[idx] + strip_byte_counts[idx])
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
                    let unpred = unpredict_buffer(decomp, predictor, 1, bits, img_w, endianness)
                        .map_err(|e| e.to_string())?;
                    if let Some(out_slice) = out_slices.get_mut(i) {
                        blit_chunk_to_window(
                            &unpred,
                            img_w,
                            strip_h,
                            1,
                            true,
                            0,
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
                let idx = s_idx;
                if idx >= strip_offsets.len() {
                    continue;
                }
                let raw = reader
                    .get_bytes(strip_offsets[idx]..strip_offsets[idx] + strip_byte_counts[idx])
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
                let unpred = unpredict_buffer(decomp, predictor, samples, bits, img_w, endianness)
                    .map_err(|e| e.to_string())?;
                blit_chunk_to_window(
                    &unpred,
                    img_w,
                    strip_h,
                    samples,
                    false,
                    0,
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
