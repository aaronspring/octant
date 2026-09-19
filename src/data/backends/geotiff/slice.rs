//! Hyperslab block slicing coordinator for TIFF/GeoTIFF datasets.

use std::collections::HashMap;
use std::sync::Arc;

use async_tiff::ImageFileDirectory;
use async_tiff::decoder::DecoderRegistry;
use async_tiff::reader::{AsyncFileReader, Endianness};
use async_tiff::tags::{PhotometricInterpretation, Predictor, SampleFormat};

use crate::data::blocks::BlockStoreError;
use crate::data::octant_block::OctantBlock;
use crate::data::slice_request::SliceRequest;

use super::blit::{ReadWindow, blit_chunk_to_window};
use super::coords::GeoSpatialBounds;
use super::decode::unpredict_buffer;
use super::palette::apply_colormap;
use super::tasks::{ChunkTarget, build_chunk_tasks, parse_slice_request};

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
            for ch in 0..3 {
                let start = ch * plane_len;
                let end = start + plane_len;
                let rgb_plane = apply_colormap(&idx_buf, cmap, bits, ch);
                buffer[start..end].copy_from_slice(&rgb_plane);
            }
        } else {
            let (mut band_slices, target_bands): (Vec<&mut [f32]>, Vec<usize>) = buffer
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
                &target_bands,
                &mut band_slices,
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

    let tasks = build_chunk_tasks(ifd, win, target_bands)?;

    for task in tasks {
        let raw = reader
            .get_bytes(task.offset..task.offset + task.byte_count)
            .await
            .map_err(|e| e.to_string())?;

        let decomp = decoder
            .decode_tile(
                raw,
                ifd.photometric_interpretation(),
                ifd.jpeg_tables(),
                task.samples_in_chunk as u16,
                bits,
                None,
            )
            .map_err(|e| e.to_string())?;

        let unpred = unpredict_buffer(
            decomp,
            predictor,
            task.samples_in_chunk,
            bits,
            task.chunk_w,
            endianness,
        )
        .map_err(|e| e.to_string())?;

        match task.target {
            ChunkTarget::Single(out_idx) => {
                if let Some(out_slice) = out_slices.get_mut(out_idx) {
                    blit_chunk_to_window(
                        &unpred,
                        task.chunk_w,
                        task.chunk_h,
                        1,
                        true,
                        task.orig_x,
                        task.orig_y,
                        sample_fmt,
                        bits,
                        is_white_zero,
                        win,
                        &[0],
                        &mut [out_slice],
                    );
                }
            }
            ChunkTarget::All => {
                blit_chunk_to_window(
                    &unpred,
                    task.chunk_w,
                    task.chunk_h,
                    task.samples_in_chunk,
                    false,
                    task.orig_x,
                    task.orig_y,
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
