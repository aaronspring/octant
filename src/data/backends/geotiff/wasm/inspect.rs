//! WebAssembly remote COG metadata inspection.

use std::sync::Arc;

use crate::data::DatasetMetadata;
use crate::data::blocks::BlockStore;

use super::super::reader::WasmHttpTiffReader;
use super::super::store::GeoTiffBlockStore;
use super::store::WasmGeoTiffBlockStore;

/// Asynchronously inspects a remote GeoTIFF/COG URL in the browser and returns `DatasetMetadata`.
pub async fn inspect_wasm_remote_geotiff(url: &str) -> Result<DatasetMetadata, String> {
    let clean_url = url.trim();
    if clean_url.is_empty() {
        return Err("URL is empty".to_string());
    }

    let store = WasmGeoTiffBlockStore::get_or_create(clean_url);

    let already_loaded = {
        let guard = store.inner.read().unwrap_or_else(|p| p.into_inner());
        guard
            .as_ref()
            .map(|s| s.inspect().map_err(|e| e.to_string()))
    };
    if let Some(res) = already_loaded {
        return res;
    }

    let reader = Arc::new(WasmHttpTiffReader::new(clean_url));
    let geo_store = GeoTiffBlockStore::from_reader(clean_url, reader)
        .await
        .map_err(|e| e.to_string())?;

    let meta = geo_store.inspect().map_err(|e| e.to_string())?;

    let mut guard = store.inner.write().unwrap_or_else(|p| p.into_inner());
    *guard = Some(geo_store);

    Ok(meta)
}
