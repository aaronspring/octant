//! Tests for the GeoTIFF and TIFF backend.

use super::store::GeoTiffBlockStore;
use crate::data::blocks::BlockStore;
use crate::data::slice_request::SliceRequest;
use std::path::PathBuf;

fn fixture_path(relative: &str) -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("dev/async-tiff/fixtures").join(relative)
}

#[test]
fn test_open_and_inspect_striped_geotiff() {
    let path = fixture_path("image-tiff/geo-5b.tif");
    if !path.exists() {
        eprintln!("Skipping fixture test, file not found: {:?}", path);
        return;
    }

    let path_str = path.to_str().expect("valid path string");
    let store = GeoTiffBlockStore::open(path_str).expect("open geotiff");

    assert_eq!(store.backend_name(), "GeoTIFF");

    let vars = store.variables().expect("list variables");
    assert!(vars.contains(&"band_1".to_string()));
    assert!(vars.contains(&"raster".to_string()));

    let meta = store.inspect().expect("inspect metadata");
    assert_eq!(meta.store_type, "GeoTIFF");

    let band1 = meta
        .variables
        .iter()
        .find(|v| v.name == "band_1")
        .expect("find band_1");
    assert_eq!(band1.shape, vec![10, 10]);
    assert_eq!(band1.dimension_names, vec!["y", "x"]);

    let raster = meta
        .variables
        .iter()
        .find(|v| v.name == "raster")
        .expect("find raster");
    assert_eq!(raster.shape, vec![5, 10, 10]);
    assert_eq!(raster.dimension_names, vec!["band", "y", "x"]);
}

#[test]
fn test_fetch_block_striped_geotiff() {
    let path = fixture_path("image-tiff/geo-5b.tif");
    if !path.exists() {
        return;
    }

    let path_str = path.to_str().expect("valid path string");
    let store = GeoTiffBlockStore::open(path_str).expect("open geotiff");

    // Single band fetch
    let req = SliceRequest::full_range("band_1", &[10, 10]);
    let block = store.fetch_block(&req).expect("fetch band_1 block");

    assert_eq!(block.shape, vec![10, 10]);
    assert_eq!(block.values.len(), 100);
    assert_eq!(block.dimension_names, vec!["y", "x"]);

    // Multi-band raster fetch
    let req_raster = SliceRequest::full_range("raster", &[5, 10, 10]);
    let block_raster = store.fetch_block(&req_raster).expect("fetch raster block");

    assert_eq!(block_raster.shape, vec![5, 10, 10]);
    assert_eq!(block_raster.values.len(), 500);
    assert_eq!(block_raster.dimension_names, vec!["band", "y", "x"]);
}

#[test]
fn test_open_and_fetch_tiled_tiff() {
    let path = fixture_path("image-tiff/tiled-rgb-u8.tif");
    if !path.exists() {
        return;
    }

    let path_str = path.to_str().expect("valid path string");
    let store = GeoTiffBlockStore::open(path_str).expect("open tiled tiff");

    let meta = store.inspect().expect("inspect");
    assert!(meta.variables.iter().any(|v| v.name == "band_1"));

    let req = SliceRequest::full_range("band_1", &[32, 32]);
    let block = store
        .fetch_block(&req)
        .expect("fetch block from tiled tiff");

    assert!(!block.values.is_empty());
}

#[test]
fn test_open_and_fetch_single_band_int16() {
    let path = fixture_path("image-tiff/int16.tif");
    if !path.exists() {
        return;
    }

    let path_str = path.to_str().expect("valid path string");
    let store = GeoTiffBlockStore::open(path_str).expect("open int16 tiff");

    let vars = store.variables().expect("list variables");
    assert_eq!(vars, vec!["band_1".to_string()]);

    let meta = store.inspect().expect("inspect");
    let band = &meta.variables[0];
    assert_eq!(band.shape, vec![64, 64]);

    let req = SliceRequest::full_range("band_1", &[64, 64]);
    let block = store.fetch_block(&req).expect("fetch int16 block");
    assert_eq!(block.values.len(), 64 * 64);
}

#[test]
fn test_open_from_bytes_and_source_factory() {
    let path = fixture_path("image-tiff/geo-5b.tif");
    if !path.exists() {
        return;
    }

    let file_bytes = std::fs::read(&path).expect("read fixture bytes");
    let rt = crate::utils::executor::get_shared_tokio_rt();

    let store = rt
        .block_on(async { GeoTiffBlockStore::from_bytes("in_memory.tif", file_bytes).await })
        .expect("open from bytes");

    let vars = store.variables().expect("variables");
    assert!(vars.contains(&"band_1".to_string()));

    // Test SourceFactory
    let path_str = path.to_str().expect("valid path");
    let source = crate::data::DataSource::new(
        "test_src",
        crate::data::DataSourceKind::GeoTiff,
        path_str,
        "Sample GeoTIFF",
    );
    let handle = crate::data::SourceFactory::open(source).expect("open through SourceFactory");
    let meta = handle.inspect().expect("inspect through SourceFactory");
    assert_eq!(meta.store_type, "GeoTIFF");
}
