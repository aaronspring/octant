//! Chunk task planning and slice request parsing for TIFF/GeoTIFF reading.

use async_tiff::ImageFileDirectory;
use async_tiff::tags::{PhotometricInterpretation, PlanarConfiguration};

use crate::data::blocks::BlockStoreError;
use crate::data::slice_request::SliceRequest;

use super::blit::ReadWindow;

/// Specifies how decoded chunk samples map to output slice buffers.
#[derive(Debug, Clone, Copy)]
pub enum ChunkTarget {
    /// A single band slice at index `usize` (e.g. for planar TIFFs).
    Single(usize),
    /// All requested bands (for interleaved/chunky TIFFs).
    All,
}

/// A descriptor representing a single chunk (tile or strip) to fetch and decompress.
#[derive(Debug, Clone)]
pub struct ChunkTask {
    pub offset: u64,
    pub byte_count: u64,
    pub orig_x: usize,
    pub orig_y: usize,
    pub chunk_w: usize,
    pub chunk_h: usize,
    pub samples_in_chunk: usize,
    pub target: ChunkTarget,
}

/// Build chunk fetch tasks for reading a region from an IFD.
pub fn build_chunk_tasks(
    ifd: &ImageFileDirectory,
    win: &ReadWindow,
    target_bands: &[usize],
) -> Result<Vec<ChunkTask>, BlockStoreError> {
    let img_w = ifd.image_width() as usize;
    let img_h = ifd.image_height() as usize;
    let is_planar = ifd.planar_configuration() == PlanarConfiguration::Planar;
    let samples = ifd.samples_per_pixel() as usize;

    let mut tasks = Vec::new();

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
                        if idx < tile_offsets.len() {
                            tasks.push(ChunkTask {
                                offset: tile_offsets[idx],
                                byte_count: tile_byte_counts[idx],
                                orig_x,
                                orig_y,
                                chunk_w: tw,
                                chunk_h: th,
                                samples_in_chunk: 1,
                                target: ChunkTarget::Single(i),
                            });
                        }
                    }
                } else {
                    let idx = ty * tiles_x + tx;
                    if idx < tile_offsets.len() {
                        tasks.push(ChunkTask {
                            offset: tile_offsets[idx],
                            byte_count: tile_byte_counts[idx],
                            orig_x,
                            orig_y,
                            chunk_w: tw,
                            chunk_h: th,
                            samples_in_chunk: samples,
                            target: ChunkTarget::All,
                        });
                    }
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
                    if idx < strip_offsets.len() {
                        tasks.push(ChunkTask {
                            offset: strip_offsets[idx],
                            byte_count: strip_byte_counts[idx],
                            orig_x: 0,
                            orig_y,
                            chunk_w: img_w,
                            chunk_h: strip_h,
                            samples_in_chunk: 1,
                            target: ChunkTarget::Single(i),
                        });
                    }
                }
            } else {
                let idx = s_idx;
                if idx < strip_offsets.len() {
                    tasks.push(ChunkTask {
                        offset: strip_offsets[idx],
                        byte_count: strip_byte_counts[idx],
                        orig_x: 0,
                        orig_y,
                        chunk_w: img_w,
                        chunk_h: strip_h,
                        samples_in_chunk: samples,
                        target: ChunkTarget::All,
                    });
                }
            }
        }
    }

    Ok(tasks)
}

/// Parse a `SliceRequest` into band index and (row_range, col_range) pixel coordinates.
pub fn parse_slice_request(
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
