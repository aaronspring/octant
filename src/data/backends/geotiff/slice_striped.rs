//! Striped hyperslab reading for GeoTIFF rasters.

use async_tiff::ImageFileDirectory;
use async_tiff::reader::AsyncFileReader;
use async_tiff::tags::{PlanarConfiguration, Predictor, SampleFormat};

use crate::data::blocks::BlockStoreError;

use super::decompress::decompress_chunk;
use super::predictor::unpredict_buffer;
use super::slice::{ReadWindow, is_nodata};
use super::unpack::unpack_samples_to_f32;

pub async fn read_striped_region(
    ifd: &ImageFileDirectory,
    reader: &dyn AsyncFileReader,
    win: &ReadWindow,
    out: &mut [f32],
) -> Result<(), BlockStoreError> {
    let rps = ifd.rows_per_strip().unwrap_or(ifd.image_height()) as usize;
    let img_w = ifd.image_width() as usize;
    let img_h = ifd.image_height() as usize;
    let out_w = win.col_end - win.col_start;

    let strip_offsets = ifd.strip_offsets().ok_or("Missing strip offsets")?;
    let strip_byte_counts = ifd.strip_byte_counts().ok_or("Missing strip byte counts")?;

    let is_planar = ifd.planar_configuration() == PlanarConfiguration::Planar;
    let samples = ifd.samples_per_pixel() as usize;
    let strips_per_band = img_h.div_ceil(rps);

    let strip_0 = win.row_start / rps;
    let strip_1 = (win.row_end.saturating_sub(1)) / rps;

    let bits = ifd.bits_per_sample().first().copied().unwrap_or(8);
    let sample_fmt = ifd
        .sample_format()
        .first()
        .copied()
        .unwrap_or(SampleFormat::Uint);
    let predictor = ifd.predictor().unwrap_or(Predictor::None);
    let is_be = ifd.endianness() == async_tiff::reader::Endianness::BigEndian;

    for s_idx in strip_0..=strip_1.min(strips_per_band.saturating_sub(1)) {
        let global_strip_idx = if is_planar {
            win.band * strips_per_band + s_idx
        } else {
            s_idx
        };
        if global_strip_idx >= strip_offsets.len() {
            break;
        }

        let offset = strip_offsets[global_strip_idx];
        let count = strip_byte_counts[global_strip_idx];
        let raw_bytes = reader
            .get_bytes(offset..offset + count)
            .await
            .map_err(|e| e.to_string())?;

        let strip_rows = rps.min(img_h.saturating_sub(s_idx * rps));
        let decomp = decompress_chunk(&raw_bytes, ifd.compression(), ifd.jpeg_tables())
            .map_err(|e| e.to_string())?;
        let unpred = unpredict_buffer(
            decomp,
            predictor,
            if is_planar { 1 } else { samples },
            bits,
            img_w,
            is_be,
        )
        .map_err(|e| e.to_string())?;
        let strip_samples = unpack_samples_to_f32(
            &unpred,
            bits,
            sample_fmt,
            ifd.photometric_interpretation(),
            img_w * (if is_planar { 1 } else { samples }),
            strip_rows,
        );

        let s_origin_y = s_idx * rps;
        let r_min = win.row_start.max(s_origin_y);
        let r_max = win.row_end.min(s_origin_y + strip_rows);

        for r in r_min..r_max {
            let local_r = r - s_origin_y;
            let dst_r = r - win.row_start;
            for c in win.col_start..win.col_end {
                let local_c = c;
                let dst_c = c - win.col_start;
                let s_sample_idx = if is_planar {
                    local_r * img_w + local_c
                } else {
                    local_r * img_w * samples + local_c * samples + win.band
                };
                let val = strip_samples.get(s_sample_idx).copied().unwrap_or(f32::NAN);
                out[dst_r * out_w + dst_c] = if is_nodata(val, win.nodata_val) {
                    f32::NAN
                } else {
                    val
                };
            }
        }
    }
    Ok(())
}
