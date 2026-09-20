use async_tiff::decoder::{Decoder, DecoderRegistry};
use async_tiff::error::{AsyncTiffError, AsyncTiffResult};
use async_tiff::reader::Endianness;
use async_tiff::tags::{Compression, PhotometricInterpretation, Predictor};
use bytes::Bytes;

/// Create a DecoderRegistry populated with standard + robust decoders.
pub fn create_decoder_registry() -> DecoderRegistry {
    let mut registry = DecoderRegistry::default();
    let map = registry.as_mut();
    map.insert(Compression::PackBits, Box::new(PackBitsDecoder));
    map.insert(Compression::LZW, Box::new(RobustLzwDecoder));
    map.insert(Compression::JPEG, Box::new(JpegDecoder));
    map.insert(Compression::ModernJPEG, Box::new(JpegDecoder));
    registry
}

/// Reverse pre-compression predictors and normalize byte endianness.
pub fn unpredict_buffer(
    mut buf: Vec<u8>,
    predictor: Predictor,
    samples: usize,
    bits: u16,
    width: usize,
    endianness: Endianness,
) -> Result<Vec<u8>, String> {
    if endianness == Endianness::BigEndian {
        match bits {
            16 => buf.chunks_exact_mut(2).for_each(|c| c.swap(0, 1)),
            32 => buf.chunks_exact_mut(4).for_each(|c| c.reverse()),
            64 => buf.chunks_exact_mut(8).for_each(|c| c.reverse()),
            _ => {}
        }
    }
    match predictor {
        Predictor::Horizontal => unpredict_horizontal(&mut buf, samples, bits, width)?,
        Predictor::FloatingPoint => return unpredict_float(buf, samples, bits, width),
        _ => {}
    }
    Ok(buf)
}

macro_rules! unpred_diff {
    ($T:ident, $row:expr, $samples:expr) => {{
        let sz = std::mem::size_of::<$T>();
        let step = $samples * sz;
        for i in (step..$row.len()).step_by(sz) {
            let prev =
                $T::from_ne_bytes($row[i - step..i - step + sz].try_into().unwrap_or_default());
            let sum = $T::from_ne_bytes($row[i..i + sz].try_into().unwrap_or_default())
                .wrapping_add(prev)
                .to_ne_bytes();
            $row[i..i + sz].copy_from_slice(&sum);
        }
    }};
}

fn unpredict_horizontal(
    buf: &mut [u8],
    samples: usize,
    bits: u16,
    width: usize,
) -> Result<(), String> {
    let bytes_per_sample = (bits as usize).div_ceil(8);
    let row_stride = width * samples * bytes_per_sample;
    if row_stride == 0 {
        return Ok(());
    }
    for row in buf.chunks_mut(row_stride) {
        match bits {
            0..=8 => {
                for i in samples..row.len() {
                    row[i] = row[i].wrapping_add(row[i - samples]);
                }
            }
            9..=16 => unpred_diff!(u16, row, samples),
            17..=32 => unpred_diff!(u32, row, samples),
            33..=64 => unpred_diff!(u64, row, samples),
            _ => return Err(format!("Unsupported bits for horizontal predictor: {bits}")),
        }
    }
    Ok(())
}

fn unpredict_float(
    mut input: Vec<u8>,
    samples: usize,
    bits: u16,
    width: usize,
) -> Result<Vec<u8>, String> {
    let bps = (bits as usize) / 8;
    let row_stride = width * samples * bps;
    if row_stride == 0 {
        return Ok(input);
    }
    if !matches!(bps, 2 | 4 | 8) {
        return Err(format!("Unsupported float predictor bits: {bits}"));
    }
    let mut output = vec![0u8; input.len()];
    for (in_row, out_row) in input
        .chunks_mut(row_stride)
        .zip(output.chunks_mut(row_stride))
    {
        for i in samples..in_row.len() {
            in_row[i] = in_row[i].wrapping_add(in_row[i - samples]);
        }
        let plane_len = in_row.len() / bps;
        for (i, chunk) in out_row.chunks_exact_mut(bps).enumerate() {
            for (b, byte) in chunk.iter_mut().enumerate() {
                *byte = in_row[plane_len * b + i];
            }
            #[cfg(target_endian = "little")]
            chunk.reverse();
        }
    }
    Ok(output)
}

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
