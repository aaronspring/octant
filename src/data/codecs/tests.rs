use super::*;
use zarrs::array::ArrayMetadata;
use zarrs::array::BytesRepresentation;
use zarrs_codec::{BytesToBytesCodecTraits, CodecOptions};

#[test]
fn test_normalize_v3_zlib_and_shuffle() {
    let raw = serde_json::json!({
        "zarr_format": 3,
        "node_type": "array",
        "shape": [5, 3600, 7200],
        "data_type": "int16",
        "chunk_grid": {
            "name": "regular",
            "configuration": {
                "chunk_shape": [1, 1200, 2400]
            }
        },
        "chunk_key_encoding": {
            "name": "default",
            "configuration": { "separator": "/" }
        },
        "fill_value": -9999,
        "codecs": [
            { "name": "numcodecs.shuffle", "configuration": { "elementsize": 2 } },
            { "name": "numcodecs.zlib", "configuration": { "level": 1 } }
        ]
    });

    let norm = normalize_v3_array_metadata(raw);
    let codecs = norm.get("codecs").and_then(|c| c.as_array()).unwrap();
    assert_eq!(codecs.len(), 3);
    assert_eq!(codecs[0]["name"], "bytes");
    assert_eq!(codecs[1]["name"], "numcodecs.shuffle");
    assert_eq!(codecs[2]["name"], "numcodecs.zlib");

    let array_meta: Result<ArrayMetadata, _> = serde_json::from_value(norm);
    assert!(array_meta.is_ok());
}

#[test]
fn test_normalize_v3_blosc() {
    let raw = serde_json::json!({
        "zarr_format": 3,
        "node_type": "array",
        "shape": [100, 100],
        "data_type": "float32",
        "chunk_grid": {
            "name": "regular",
            "configuration": { "chunk_shape": [10, 10] }
        },
        "chunk_key_encoding": {
            "name": "default",
            "configuration": { "separator": "/" }
        },
        "fill_value": 0.0,
        "codecs": [
            {
                "name": "numcodecs.blosc",
                "configuration": {
                    "cname": "zstd",
                    "clevel": 5,
                    "shuffle": 1,
                    "blocksize": 0
                }
            }
        ]
    });

    let norm = normalize_v3_array_metadata(raw);
    let array_meta: Result<ArrayMetadata, _> = serde_json::from_value(norm);
    assert!(array_meta.is_ok());
}

#[test]
fn test_blusc_codec_round_trip() {
    let codec = BluscCodec::new();
    let options = CodecOptions::default();
    let rep = BytesRepresentation::UnboundedSize;

    let original_data: Vec<f32> = (0..256).map(|x| (x as f32) * 0.5).collect();
    let bytes: &[u8] = bytemuck::cast_slice(&original_data);

    let compressed = codec
        .encode(bytes.into(), &options)
        .expect("compression should succeed");
    assert!(!compressed.is_empty());

    let decompressed = codec
        .decode(compressed, &rep, &options)
        .expect("decompression should succeed");
    assert_eq!(decompressed.as_ref(), bytes);
}

#[test]
fn test_blusc_codec_int16_round_trip() {
    let codec = BluscCodec::new();
    let options = CodecOptions::default();
    let rep = BytesRepresentation::UnboundedSize;

    let original_data: Vec<i16> = (0..1024).map(|x| (x % 500) as i16).collect();
    let bytes: &[u8] = bytemuck::cast_slice(&original_data);

    let compressed = codec
        .encode(bytes.into(), &options)
        .expect("compression should succeed");
    assert!(!compressed.is_empty());

    let decompressed = codec
        .decode(compressed, &rep, &options)
        .expect("decompression should succeed");
    assert_eq!(decompressed.as_ref(), bytes);
}
