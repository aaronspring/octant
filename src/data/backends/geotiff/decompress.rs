//! Self-contained decompression routines for TIFF data buffers.

use async_tiff::tags::Compression;
use std::io::Read;

/// Decompress raw compressed tile/strip bytes based on the TIFF compression method.
pub fn decompress_chunk(
    compressed: &[u8],
    compression: Compression,
    jpeg_tables: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    match compression {
        Compression::None => Ok(compressed.to_vec()),
        Compression::PackBits => decompress_packbits(compressed),
        Compression::LZW => decompress_lzw(compressed),
        Compression::Deflate | Compression::OldDeflate => decompress_deflate(compressed),
        Compression::ZSTD => decompress_zstd(compressed),
        Compression::ModernJPEG | Compression::JPEG => decompress_jpeg(compressed, jpeg_tables),
        other => Err(format!("Unsupported TIFF compression: {other:?}")),
    }
}

/// Apple/TIFF PackBits byte-run RLE decompressor.
pub fn decompress_packbits(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut i = 0;
    while i < input.len() {
        let n = input[i] as i8;
        i += 1;
        if n >= 0 {
            let count = n as usize + 1;
            if i + count > input.len() {
                return Err("PackBits literal run exceeds input bounds".to_string());
            }
            out.extend_from_slice(&input[i..i + count]);
            i += count;
        } else if n != -128 {
            let count = (-n) as usize + 1;
            if i >= input.len() {
                return Err("PackBits repeated byte missing in input".to_string());
            }
            let byte = input[i];
            i += 1;
            out.resize(out.len() + count, byte);
        }
    }
    Ok(out)
}

/// LZW decompressor using weezl with TIFF MSB/LSB bit ordering and compat fallbacks.
pub fn decompress_lzw(input: &[u8]) -> Result<Vec<u8>, String> {
    // 1. Standard TIFF LZW (MSB with TIFF size switch)
    let mut decoder = weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8);
    if let Ok(data) = decoder.decode(input) {
        return Ok(data);
    }
    // 2. Standard LZW (MSB standard switch)
    let mut compat_decoder = weezl::decode::Decoder::new(weezl::BitOrder::Msb, 8);
    if let Ok(data) = compat_decoder.decode(input) {
        return Ok(data);
    }
    // 3. LSB LZW (FillOrder=2 / compat mode with TIFF switch)
    let mut lsb_decoder = weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Lsb, 8);
    if let Ok(data) = lsb_decoder.decode(input) {
        return Ok(data);
    }
    // 4. LSB LZW (FillOrder=2 / compat mode with standard switch)
    let mut lsb_std_decoder = weezl::decode::Decoder::new(weezl::BitOrder::Lsb, 8);
    lsb_std_decoder
        .decode(input)
        .map_err(|e| format!("LZW decompression failed: {e:?}"))
}

/// Deflate / Zlib decompressor.
pub fn decompress_deflate(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = flate2::read::ZlibDecoder::new(input);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| format!("Deflate decompression failed: {e}"))?;
    Ok(out)
}

/// Zstd decompressor using ruzstd.
pub fn decompress_zstd(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(input)
        .map_err(|e| format!("Zstd decoder init failed: {e:?}"))?;
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| format!("Zstd decompression failed: {e:?}"))?;
    Ok(out)
}

/// JPEG decompressor combining JPEGTables with tile/strip payload.
pub fn decompress_jpeg(input: &[u8], jpeg_tables: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let combined_data = if let Some(tables) = jpeg_tables
        && tables.len() >= 4
        && input.len() >= 2
    {
        let mut combined = Vec::with_capacity(tables.len() + input.len());
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
        combined.extend_from_slice(tables_payload);
        combined.extend_from_slice(input_payload);
        combined
    } else {
        input.to_vec()
    };

    let img = image::load_from_memory_with_format(&combined_data, image::ImageFormat::Jpeg)
        .map_err(|e| format!("JPEG decompression failed: {e}"))?;
    Ok(img.to_rgb8().into_raw())
}
