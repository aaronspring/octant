//! Pure-Rust decoder implementations for TIFF compression formats.

use async_tiff::decoder::Decoder;
use async_tiff::error::{AsyncTiffError, AsyncTiffResult};
use async_tiff::tags::PhotometricInterpretation;
use bytes::Bytes;

#[derive(Debug, Clone)]
pub struct PackBitsDecoder;

impl Decoder for PackBitsDecoder {
    fn decode_tile(
        &self,
        buf: Bytes,
        _p: PhotometricInterpretation,
        _t: Option<&[u8]>,
        _s: u16,
        _b: u16,
        _l: Option<&[u32]>,
    ) -> AsyncTiffResult<Vec<u8>> {
        let input = buf.as_ref();
        let mut out = Vec::with_capacity(input.len() * 2);
        let mut i = 0;
        while i < input.len() {
            let n = input[i] as i8;
            i += 1;
            if n >= 0 {
                let count = n as usize + 1;
                if i + count > input.len() {
                    return Err(AsyncTiffError::General("PackBits run exceeds input".into()));
                }
                out.extend_from_slice(&input[i..i + count]);
                i += count;
            } else if n != -128 {
                let count = (-n) as usize + 1;
                if i >= input.len() {
                    return Err(AsyncTiffError::General("PackBits byte missing".into()));
                }
                out.resize(out.len() + count, input[i]);
                i += 1;
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct RobustLzwDecoder;

impl Decoder for RobustLzwDecoder {
    fn decode_tile(
        &self,
        buf: Bytes,
        _p: PhotometricInterpretation,
        _t: Option<&[u8]>,
        _s: u16,
        _b: u16,
        _l: Option<&[u32]>,
    ) -> AsyncTiffResult<Vec<u8>> {
        let input = buf.as_ref();
        if let Ok(data) =
            weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8).decode(input)
        {
            return Ok(data);
        }
        if let Ok(data) = weezl::decode::Decoder::new(weezl::BitOrder::Msb, 8).decode(input) {
            return Ok(data);
        }
        if let Ok(data) =
            weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Lsb, 8).decode(input)
        {
            return Ok(data);
        }
        weezl::decode::Decoder::new(weezl::BitOrder::Lsb, 8)
            .decode(input)
            .map_err(|e| AsyncTiffError::General(format!("LZW decompression failed: {e:?}")))
    }
}

#[derive(Debug, Clone)]
pub struct JpegDecoder;

impl Decoder for JpegDecoder {
    fn decode_tile(
        &self,
        buf: Bytes,
        _p: PhotometricInterpretation,
        tables: Option<&[u8]>,
        _s: u16,
        _b: u16,
        _l: Option<&[u32]>,
    ) -> AsyncTiffResult<Vec<u8>> {
        let input = buf.as_ref();
        let combined = if let Some(t) = tables
            && t.len() >= 4
            && input.len() >= 2
        {
            let mut c = Vec::with_capacity(t.len() + input.len());
            let t_body = if t.ends_with(&[0xFF, 0xD9]) {
                &t[..t.len() - 2]
            } else {
                t
            };
            let in_body = if input.starts_with(&[0xFF, 0xD8]) {
                &input[2..]
            } else {
                input
            };
            c.extend_from_slice(t_body);
            c.extend_from_slice(in_body);
            c
        } else {
            input.to_vec()
        };
        let img = image::load_from_memory_with_format(&combined, image::ImageFormat::Jpeg)
            .map_err(|e| AsyncTiffError::General(format!("JPEG decode failed: {e}")))?;
        Ok(img.to_rgb8().into_raw())
    }
}
