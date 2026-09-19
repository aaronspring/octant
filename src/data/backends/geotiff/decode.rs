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
        swap_endianness(&mut buf, bits);
    }
    match predictor {
        Predictor::Horizontal => unpredict_horizontal(&mut buf, samples, bits, width)?,
        Predictor::FloatingPoint => return unpredict_float(buf, samples, bits, width),
        _ => {}
    }
    Ok(buf)
}

fn swap_endianness(buf: &mut [u8], bits: u16) {
    match bits {
        16 => {
            for c in buf.chunks_exact_mut(2) {
                c.swap(0, 1);
            }
        }
        32 => {
            for c in buf.chunks_exact_mut(4) {
                c.reverse();
            }
        }
        64 => {
            for c in buf.chunks_exact_mut(8) {
                c.reverse();
            }
        }
        _ => {}
    }
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
            9..=16 => {
                let step = samples * 2;
                for i in (step..row.len()).step_by(2) {
                    let prev = u16::from_ne_bytes([row[i - step], row[i - step + 1]]);
                    let curr = u16::from_ne_bytes([row[i], row[i + 1]]);
                    let sum = curr.wrapping_add(prev).to_ne_bytes();
                    row[i] = sum[0];
                    row[i + 1] = sum[1];
                }
            }
            17..=32 => {
                let step = samples * 4;
                for i in (step..row.len()).step_by(4) {
                    let prev = u32::from_ne_bytes(
                        row[i - step..i - step + 4].try_into().unwrap_or_default(),
                    );
                    let curr = u32::from_ne_bytes(row[i..i + 4].try_into().unwrap_or_default());
                    let sum = curr.wrapping_add(prev).to_ne_bytes();
                    row[i..i + 4].copy_from_slice(&sum);
                }
            }
            33..=64 => {
                let step = samples * 8;
                for i in (step..row.len()).step_by(8) {
                    let prev = u64::from_ne_bytes(
                        row[i - step..i - step + 8].try_into().unwrap_or_default(),
                    );
                    let curr = u64::from_ne_bytes(row[i..i + 8].try_into().unwrap_or_default());
                    let sum = curr.wrapping_add(prev).to_ne_bytes();
                    row[i..i + 8].copy_from_slice(&sum);
                }
            }
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
    let bytes_per_sample = (bits as usize) / 8;
    let row_stride = width * samples * bytes_per_sample;
    if row_stride == 0 {
        return Ok(input);
    }
    let mut output = vec![0u8; input.len()];
    for (in_row, out_row) in input
        .chunks_mut(row_stride)
        .zip(output.chunks_mut(row_stride))
    {
        for i in samples..in_row.len() {
            in_row[i] = in_row[i].wrapping_add(in_row[i - samples]);
        }
        let plane_len = in_row.len() / bytes_per_sample;
        match bytes_per_sample {
            2 => {
                for (i, chunk) in out_row.chunks_exact_mut(2).enumerate() {
                    let b = u16::from_be_bytes([in_row[i], in_row[plane_len + i]]).to_ne_bytes();
                    chunk.copy_from_slice(&b);
                }
            }
            4 => {
                for (i, chunk) in out_row.chunks_exact_mut(4).enumerate() {
                    let b = u32::from_be_bytes([
                        in_row[i],
                        in_row[plane_len + i],
                        in_row[plane_len * 2 + i],
                        in_row[plane_len * 3 + i],
                    ])
                    .to_ne_bytes();
                    chunk.copy_from_slice(&b);
                }
            }
            8 => {
                for (i, chunk) in out_row.chunks_exact_mut(8).enumerate() {
                    let arr: [u8; 8] = std::array::from_fn(|idx| in_row[plane_len * idx + i]);
                    let b = u64::from_be_bytes(arr).to_ne_bytes();
                    chunk.copy_from_slice(&b);
                }
            }
            _ => return Err(format!("Unsupported float predictor bits: {bits}")),
        }
    }
    Ok(output)
}

/// A decoder for the PackBits compression method.
#[derive(Debug, Clone)]
pub struct PackBitsDecoder;

impl Decoder for PackBitsDecoder {
    fn decode_tile(
        &self,
        buffer: Bytes,
        _photo: PhotometricInterpretation,
        _tables: Option<&[u8]>,
        _samples: u16,
        _bits: u16,
        _lerc: Option<&[u32]>,
    ) -> AsyncTiffResult<Vec<u8>> {
        let input = buffer.as_ref();
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
                let byte = input[i];
                i += 1;
                out.resize(out.len() + count, byte);
            }
        }
        Ok(out)
    }
}

/// A robust LZW decoder with TIFF size switch and LSB compat fallbacks.
#[derive(Debug, Clone)]
pub struct RobustLzwDecoder;

impl Decoder for RobustLzwDecoder {
    fn decode_tile(
        &self,
        buffer: Bytes,
        _photo: PhotometricInterpretation,
        _tables: Option<&[u8]>,
        _samples: u16,
        _bits: u16,
        _lerc: Option<&[u32]>,
    ) -> AsyncTiffResult<Vec<u8>> {
        let input = buffer.as_ref();
        let mut decoder = weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8);
        if let Ok(data) = decoder.decode(input) {
            return Ok(data);
        }
        let mut compat = weezl::decode::Decoder::new(weezl::BitOrder::Msb, 8);
        if let Ok(data) = compat.decode(input) {
            return Ok(data);
        }
        let mut lsb = weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Lsb, 8);
        if let Ok(data) = lsb.decode(input) {
            return Ok(data);
        }
        let mut lsb_std = weezl::decode::Decoder::new(weezl::BitOrder::Lsb, 8);
        lsb_std
            .decode(input)
            .map_err(|e| AsyncTiffError::General(format!("LZW decompression failed: {e:?}")))
    }
}

/// A JPEG decoder combining JPEGTables with tile payload.
#[derive(Debug, Clone)]
pub struct JpegDecoder;

impl Decoder for JpegDecoder {
    fn decode_tile(
        &self,
        buffer: Bytes,
        _photo: PhotometricInterpretation,
        jpeg_tables: Option<&[u8]>,
        _samples: u16,
        _bits: u16,
        _lerc: Option<&[u32]>,
    ) -> AsyncTiffResult<Vec<u8>> {
        let input = buffer.as_ref();
        let combined = if let Some(tables) = jpeg_tables
            && tables.len() >= 4
            && input.len() >= 2
        {
            let mut c = Vec::with_capacity(tables.len() + input.len());
            let tables_payload = if tables.ends_with(&[0xFF, 0xD9]) {
                &tables[..tables.len() - 2]
            } else {
                tables
            };
            let input_payload = if input.starts_with(&[0xFF, 0xD8]) {
                &input[2..]
            } else {
                input
            };
            c.extend_from_slice(tables_payload);
            c.extend_from_slice(input_payload);
            c
        } else {
            input.to_vec()
        };

        let img = image::load_from_memory_with_format(&combined, image::ImageFormat::Jpeg)
            .map_err(|e| AsyncTiffError::General(format!("JPEG decode failed: {e}")))?;
        Ok(img.to_rgb8().into_raw())
    }
}
