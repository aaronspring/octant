use icechunk_format::format_constants::{
    CompressionAlgorithmBin, ICECHUNK_FILE_HEADER_LEN, ICECHUNK_FORMAT_MAGIC_BYTES, SpecVersionBin,
};

use super::header::decompress_icechunk_file;

#[test]
fn test_decompress_empty_and_raw() {
    let empty: &[u8] = &[];
    let (spec, decomp) = decompress_icechunk_file(empty).expect("empty succeeds");
    assert_eq!(spec, SpecVersionBin::V1);
    assert!(decomp.is_empty());

    let raw = b"hello uncompressed icechunk data";
    let (spec, decomp) = decompress_icechunk_file(raw).expect("raw succeeds");
    assert_eq!(spec, SpecVersionBin::V1);
    assert_eq!(decomp, raw);
}

#[test]
fn test_decompress_with_39_byte_header_uncompressed() {
    // Build 39-byte header: magic(12) + impl(24) + spec(1) + file_type(1) + comp(1)
    let mut data = Vec::with_capacity(ICECHUNK_FILE_HEADER_LEN + 10);
    data.extend_from_slice(ICECHUNK_FORMAT_MAGIC_BYTES); // 12 bytes
    data.extend_from_slice(b"ic-2.1.2                "); // 24 bytes
    data.push(SpecVersionBin::V2 as u8); // spec version 2
    data.push(4); // file type RepoInfo
    data.push(CompressionAlgorithmBin::None as u8); // uncompressed
    data.extend_from_slice(b"sample-uncompressed-body");

    assert_eq!(data.len(), ICECHUNK_FILE_HEADER_LEN + 24);

    let (spec, decomp) = decompress_icechunk_file(&data).expect("header parse succeeds");
    assert_eq!(spec, SpecVersionBin::V2);
    assert_eq!(decomp, b"sample-uncompressed-body");
}

#[test]
fn test_v1_ref_json_parsing() {
    let json_data = r#"{"snapshot": "000G40R40M30E209185G"}"#;
    let val = serde_json::from_str::<serde_json::Value>(json_data).expect("json parse");
    let snap_id = val
        .get("snapshot")
        .and_then(|v| v.as_str())
        .expect("snapshot field");
    assert_eq!(snap_id, "000G40R40M30E209185G");
}
