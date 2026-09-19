//! Tiled hyperslab reading for GeoTIFF rasters.

use async_tiff::ImageFileDirectory;
use async_tiff::reader::AsyncFileReader;
use async_tiff::tags::{PlanarConfiguration, Predictor, SampleFormat};

use crate::data::blocks::BlockStoreError;

use super::decompress::decompress_chunk;
use super::predictor::unpredict_buffer;
use super::slice::{ReadWindow, is_nodata};
use super::unpack::unpack_samples_to_f32;

pub async fn read_tiled_region(
    ifd: &ImageFileDirectory,
    reader: &dyn AsyncFileReader,
    win: &ReadWindow,
    out: &mut [f32],
) -> Result<(), BlockStoreError> {
    let tw = ifd.tile_width().unwrap_or(ifd.image_width()) as usize;
    let th = ifd.tile_height().unwrap_or(ifd.image_height()) as usize;
    let out_w = win.col_end - win.col_start;

    let tile_offsets = ifd.tile_offsets().ok_or("Missing tile offsets")?;
    let tile_byte_counts = ifd.tile_byte_counts().ok_or("Missing tile byte counts")?;
    let (tiles_x, tiles_y) = ifd.tile_count().unwrap_or((1, 1));

    let tx0 = win.col_start / tw;
    let tx1 = (win.col_end.saturating_sub(1)) / tw;
    let ty0 = win.row_start / th;
    let ty1 = (win.row_end.saturating_sub(1)) / th;

    let is_planar = ifd.planar_configuration() == PlanarConfiguration::Planar;
    let samples = ifd.samples_per_pixel() as usize;
    let bits = ifd.bits_per_sample().first().copied().unwrap_or(8);
    let sample_fmt = ifd
        .sample_format()
        .first()
        .copied()
        .unwrap_or(SampleFormat::Uint);
    let predictor = ifd.predictor().unwrap_or(Predictor::None);
    let is_be = ifd.endianness() == async_tiff::reader::Endianness::BigEndian;

    for ty in ty0..=ty1.min(tiles_y.saturating_sub(1)) {
        for tx in tx0..=tx1.min(tiles_x.saturating_sub(1)) {
            let tile_idx = if is_planar {
                win.band * (tiles_x * tiles_y) + ty * tiles_x + tx
            } else {
                ty * tiles_x + tx
            };
            if tile_idx >= tile_offsets.len() {
                continue;
            }
            let offset = tile_offsets[tile_idx];
            let count = tile_byte_counts[tile_idx];
            let raw_bytes = reader
                .get_bytes(offset..offset + count)
                .await
                .map_err(|e| e.to_string())?;

            let decomp = decompress_chunk(&raw_bytes, ifd.compression(), ifd.jpeg_tables())
                .map_err(|e| e.to_string())?;
            let unpred = unpredict_buffer(
                decomp,
                predictor,
                if is_planar { 1 } else { samples },
                bits,
                tw,
                is_be,
            )
            .map_err(|e| e.to_string())?;
            let tile_samples = unpack_samples_to_f32(
                &unpred,
                bits,
                sample_fmt,
                ifd.photometric_interpretation(),
                tw * (if is_planar { 1 } else { samples }),
                th,
            );

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
                    let s_idx = if is_planar {
                        local_r * tw + local_c
                    } else {
                        local_r * tw * samples + local_c * samples + win.band
                    };
                    let val = tile_samples.get(s_idx).copied().unwrap_or(f32::NAN);
                    out[dst_r * out_w + dst_c] = if is_nodata(val, win.nodata_val) {
                        f32::NAN
                    } else {
                        val
                    };
                }
            }
        }
    }
    Ok(())
}
