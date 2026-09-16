//! Lightweight pure-Rust Icechunk snapshot and manifest resolver for WebAssembly.
//!
//! Provides read-only streaming of Icechunk datasets directly in web browsers without
//! Tokio, MIO, or native socket dependencies.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, RwLock};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Mutex, OnceLock};

#[cfg(target_arch = "wasm32")]
use icechunk_format::ChunkIndices;
use icechunk_format::format_constants::{
    CompressionAlgorithmBin, ICECHUNK_FILE_HEADER_LEN, ICECHUNK_FORMAT_MAGIC_BYTES, SpecVersionBin,
    parse_file_header,
};
#[cfg(target_arch = "wasm32")]
use icechunk_format::manifest::ChunkPayload;
use icechunk_format::manifest::{Manifest, ManifestRef};
#[cfg(target_arch = "wasm32")]
use icechunk_format::repo_info::RepoInfo;
#[cfg(target_arch = "wasm32")]
use icechunk_format::snapshot::{NodeData, Snapshot};
use zarrs::array::ArraySubset;

use super::wasm_zarr::WasmZarrBlockStore;
#[cfg(target_arch = "wasm32")]
use super::wasm_zarr::{fetch_url_byte_range, fetch_url_bytes};
use super::zarr_block::fetch_block_with_progress;
use crate::data::DatasetMetadata;
#[cfg(target_arch = "wasm32")]
use crate::data::VariableInfo;
use crate::data::blocks::{BlockRequest, BlockStore, BlockStoreError, ProgressCallback};
use crate::data::octant_block::OctantBlock;
use crate::data::slice_request::SliceRequest;
#[cfg(target_arch = "wasm32")]
use crate::utils::metadata::{open_or_instantiate_array_normalized, variable_info_from_array};
#[cfg(target_arch = "wasm32")]
use crate::utils::units::calculate_variable_size_bytes;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WASM_ICECHUNK_STORES: std::cell::RefCell<HashMap<String, Arc<WasmIcechunkBlockStore>>> =
        std::cell::RefCell::new(HashMap::new());
}

#[cfg(not(target_arch = "wasm32"))]
static WASM_ICECHUNK_STORES_DESKTOP: OnceLock<Mutex<HashMap<String, Arc<WasmIcechunkBlockStore>>>> =
    OnceLock::new();

/// Converts an S3 or Virtual Chunk Container URI into an HTTPS URL for browser fetching.
pub fn s3_to_https(location: &str) -> String {
    if let Some(rest) = location.strip_prefix("s3://") {
        if let Some((bucket, key)) = rest.split_once('/') {
            format!("https://{bucket}.s3.amazonaws.com/{key}")
        } else {
            format!("https://{rest}.s3.amazonaws.com")
        }
    } else if let Some(rest) = location.strip_prefix("vcc://") {
        if let Some((container, key)) = rest.split_once('/') {
            format!("https://{container}.s3.amazonaws.com/{key}")
        } else {
            format!("https://{rest}.s3.amazonaws.com")
        }
    } else {
        location.to_string()
    }
}

fn try_decompress_zstd(data: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(data).ok()?;
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).ok()?;
    Some(out)
}

/// Decompresses an Icechunk binary file, respecting the 39-byte header and spec version.
pub fn decompress_icechunk_file(data: &[u8]) -> Result<(SpecVersionBin, Vec<u8>), BlockStoreError> {
    if data.is_empty() {
        return Ok((SpecVersionBin::V1, Vec::new()));
    }

    // 1. Check for standard 39-byte Icechunk binary file header
    if data.len() >= ICECHUNK_FILE_HEADER_LEN
        && data.starts_with(ICECHUNK_FORMAT_MAGIC_BYTES)
        && let Ok(header) = parse_file_header(data)
    {
        let payload = &data[ICECHUNK_FILE_HEADER_LEN..];
        let decompressed = match header.compression {
            CompressionAlgorithmBin::Zstd => try_decompress_zstd(payload).ok_or_else(|| {
                BlockStoreError::from("Failed to decompress Zstd payload from Icechunk file")
            })?,
            CompressionAlgorithmBin::None => payload.to_vec(),
        };
        return Ok((header.spec_version, decompressed));
    }

    // 2. Direct Zstandard fallback if raw compressed stream
    if let Some(decompressed) = try_decompress_zstd(data) {
        return Ok((SpecVersionBin::V1, decompressed));
    }

    // 3. Fall back to raw payload if uncompressed
    Ok((SpecVersionBin::V1, data.to_vec()))
}

/// Decompresses raw Icechunk payloads (FlatBuffers or Zstandard).
pub fn decompress_payload(data: &[u8]) -> Result<Vec<u8>, BlockStoreError> {
    decompress_icechunk_file(data).map(|(_, bytes)| bytes)
}

pub struct ArrayManifestInfo {
    pub node_id: icechunk_format::ObjectId<8, icechunk_format::NodeTag>,
    pub manifests: Vec<ManifestRef>,
}

pub struct WasmIcechunkBlockStore {
    pub base_url: String,
    pub inner: Arc<WasmZarrBlockStore>,
    pub array_manifests: RwLock<HashMap<String, ArrayManifestInfo>>,
    pub cached_manifests: RwLock<HashMap<String, Arc<Manifest>>>,
}

impl WasmIcechunkBlockStore {
    #[cfg(target_arch = "wasm32")]
    pub fn get_or_create(url: &str) -> Arc<Self> {
        let clean_url = url
            .trim_start_matches("icechunk+")
            .trim_end_matches('/')
            .to_string();
        WASM_ICECHUNK_STORES.with(|stores| {
            let mut map = stores.borrow_mut();
            if let Some(store) = map.get(&clean_url) {
                store.clone()
            } else {
                let inner = WasmZarrBlockStore::get_or_create(&clean_url);
                let new_store = Arc::new(Self {
                    base_url: clean_url.clone(),
                    inner,
                    array_manifests: RwLock::new(HashMap::new()),
                    cached_manifests: RwLock::new(HashMap::new()),
                });
                map.insert(clean_url, new_store.clone());
                new_store
            }
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_or_create(url: &str) -> Arc<Self> {
        let clean_url = url
            .trim_start_matches("icechunk+")
            .trim_end_matches('/')
            .to_string();
        let stores = WASM_ICECHUNK_STORES_DESKTOP.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = stores.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(store) = guard.get(&clean_url) {
            store.clone()
        } else {
            let inner = WasmZarrBlockStore::get_or_create(&clean_url);
            let new_store = Arc::new(Self {
                base_url: clean_url.clone(),
                inner,
                array_manifests: RwLock::new(HashMap::new()),
                cached_manifests: RwLock::new(HashMap::new()),
            });
            guard.insert(clean_url, new_store.clone());
            new_store
        }
    }

    /// Preloads chunks required for a slice request by querying Icechunk manifests.
    #[cfg(target_arch = "wasm32")]
    pub async fn preload_chunks_for_subset(
        &self,
        var_name: &str,
        subset: &ArraySubset,
        mut on_progress: ProgressCallback<'_>,
    ) -> Result<(), BlockStoreError> {
        let clean_var = var_name.trim_start_matches('/').to_string();
        let var_path = format!("/{clean_var}");

        let array =
            open_or_instantiate_array_normalized(self.inner.memory_store.clone(), &var_path)
                .map_err(|e| format!("Failed to open array '{clean_var}': {e}"))?;

        let rank = array.shape().len();
        let zero_idx = vec![0u64; rank];

        let chunk_dims = array
            .chunk_shape(&zero_idx)
            .map_err(|e| format!("Failed to get chunk shape: {e}"))?;

        // Retrieve manifest metadata for this array
        let (node_id, manifests) = {
            let guard = self
                .array_manifests
                .read()
                .unwrap_or_else(|p| p.into_inner());
            let info = guard.get(&clean_var).ok_or_else(|| {
                BlockStoreError::from(format!(
                    "No manifest metadata recorded for array '{clean_var}'"
                ))
            })?;
            (info.node_id.clone(), info.manifests.clone())
        };

        // Calculate chunk index ranges per dimension
        let mut chunk_ranges = Vec::with_capacity(rank);
        for (i, dim_len) in chunk_dims.iter().enumerate() {
            let c_len = dim_len.get();
            let sel_start = subset.start()[i];
            let sel_shape = subset.shape()[i];
            let sel_end = sel_start + sel_shape;

            let c_start = sel_start / c_len;
            let c_end = (sel_end.saturating_sub(1) / c_len) + 1;
            chunk_ranges.push(c_start..c_end);
        }

        // Iterate over Cartesian product of chunk indices
        let mut current = chunk_ranges.iter().map(|r| r.start).collect::<Vec<u64>>();
        loop {
            let chunk_rel_key = array.chunk_key(&current);
            let store_key_str = chunk_rel_key.as_str().to_string();

            if !self.inner.has_key(&store_key_str) {
                let coords_u32 = current.iter().map(|&x| x as u32).collect::<Vec<u32>>();
                let chunk_indices = ChunkIndices(coords_u32);

                let mut chunk_resolved = false;

                // Search manifests for chunk location
                for man_ref in &manifests {
                    let man_id_str = man_ref.object_id.to_string();

                    // Check manifest cache or fetch & decompress
                    let cached_opt = {
                        let guard = self
                            .cached_manifests
                            .read()
                            .unwrap_or_else(|p| p.into_inner());
                        guard.get(&man_id_str).cloned()
                    };

                    let manifest_arc = match cached_opt {
                        Some(m) => m,
                        None => {
                            let man_url = format!("{}/manifests/{man_id_str}", self.base_url);
                            log::info!("[WASM Icechunk] Fetching manifest: {man_url}");

                            let raw_man_bytes = fetch_url_bytes(&man_url).await.map_err(|e| {
                                format!("Failed fetching manifest '{man_url}': {e}")
                            })?;

                            let (_spec, decomp_man) = decompress_icechunk_file(&raw_man_bytes)?;
                            let decoded_man = Manifest::from_buffer(decomp_man).map_err(|e| {
                                format!("Failed parsing Icechunk manifest '{man_id_str}': {e:?}")
                            })?;

                            let arc_m = Arc::new(decoded_man);
                            let mut guard = self
                                .cached_manifests
                                .write()
                                .unwrap_or_else(|p| p.into_inner());
                            guard.insert(man_id_str.clone(), arc_m.clone());
                            arc_m
                        }
                    };

                    match manifest_arc.get_chunk_payload(&node_id, &chunk_indices) {
                        Ok(ChunkPayload::Virtual(vchunk)) => {
                            let location_url = vchunk.location.url();
                            let target_url = s3_to_https(location_url);
                            log::info!(
                                "[WASM Icechunk] Fetching virtual chunk from '{target_url}' (offset: {}, len: {})",
                                vchunk.offset,
                                vchunk.length
                            );

                            let chunk_bytes =
                                fetch_url_byte_range(&target_url, vchunk.offset, vchunk.length)
                                    .await
                                    .map_err(|e| {
                                        format!(
                                            "Failed fetching virtual chunk from '{target_url}': {e}"
                                        )
                                    })?;

                            let bytes_len = chunk_bytes.len() as u64;
                            self.inner.insert_key_bytes(&store_key_str, &chunk_bytes)?;

                            if let Some(ref mut cb) = on_progress {
                                cb(bytes_len);
                            }

                            chunk_resolved = true;
                            break;
                        }
                        Ok(ChunkPayload::Ref(rchunk)) => {
                            let target_url = format!("{}/chunks/{}", self.base_url, rchunk.id);
                            log::info!(
                                "[WASM Icechunk] Fetching chunk ref from '{target_url}' (offset: {}, len: {})",
                                rchunk.offset,
                                rchunk.length
                            );

                            let chunk_bytes =
                                fetch_url_byte_range(&target_url, rchunk.offset, rchunk.length)
                                    .await
                                    .map_err(|e| {
                                        format!(
                                            "Failed fetching chunk ref from '{target_url}': {e}"
                                        )
                                    })?;

                            let bytes_len = chunk_bytes.len() as u64;
                            self.inner.insert_key_bytes(&store_key_str, &chunk_bytes)?;

                            if let Some(ref mut cb) = on_progress {
                                cb(bytes_len);
                            }

                            chunk_resolved = true;
                            break;
                        }
                        Ok(ChunkPayload::Inline(bytes)) => {
                            let bytes_len = bytes.len() as u64;
                            self.inner.insert_key_bytes(&store_key_str, &bytes)?;

                            if let Some(ref mut cb) = on_progress {
                                cb(bytes_len);
                            }

                            chunk_resolved = true;
                            break;
                        }
                        _ => {}
                    }
                }

                if !chunk_resolved {
                    log::debug!(
                        "[WASM Icechunk] Chunk {store_key_str} not in manifests (sparse fill)"
                    );
                }
            }

            // Advance chunk indices
            let mut carry = true;
            for i in (0..rank).rev() {
                current[i] += 1;
                if current[i] < chunk_ranges[i].end {
                    carry = false;
                    break;
                }
                current[i] = chunk_ranges[i].start;
            }
            if carry {
                break;
            }
        }

        // Preload coordinate chunks
        let dim_names = crate::utils::resolve_array_dimension_names(&array);
        self.inner.preload_coordinate_chunks(&dim_names).await;

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn preload_chunks_for_subset(
        &self,
        _var_name: &str,
        _subset: &ArraySubset,
        _on_progress: ProgressCallback<'_>,
    ) -> Result<(), BlockStoreError> {
        Ok(())
    }
}

impl BlockStore for WasmIcechunkBlockStore {
    fn backend_name(&self) -> &str {
        "Icechunk (Web HTTP)"
    }

    fn variables(&self) -> Result<Vec<String>, BlockStoreError> {
        self.inner.variables()
    }

    fn inspect(&self) -> Result<DatasetMetadata, BlockStoreError> {
        self.inner.inspect()
    }

    fn fetch_block(&self, request: &SliceRequest) -> Result<OctantBlock, BlockStoreError> {
        fetch_block_with_progress(
            self.inner.memory_store.clone(),
            &self.base_url,
            request,
            None,
        )
    }

    fn fetch_block_with_progress(
        &self,
        request: &SliceRequest,
        on_progress: ProgressCallback,
    ) -> Result<OctantBlock, BlockStoreError> {
        fetch_block_with_progress(
            self.inner.memory_store.clone(),
            &self.base_url,
            request,
            on_progress,
        )
    }
}

/// Asynchronously inspects a remote Icechunk repository in the browser and returns `DatasetMetadata`.
#[cfg(target_arch = "wasm32")]
pub async fn inspect_wasm_remote_icechunk(url: &str) -> Result<DatasetMetadata, String> {
    let clean_url = url.trim_start_matches("icechunk+").trim_end_matches('/');
    if clean_url.is_empty() {
        return Err("URL is empty".to_string());
    }

    let store = WasmIcechunkBlockStore::get_or_create(clean_url);

    // 1. Attempt Icechunk V2 discovery via RepoInfo (/repo)
    let repo_url = format!("{clean_url}/repo");
    let mut resolved_snap_id: Option<(String, SpecVersionBin)> = None;

    if let Ok(raw_repo_bytes) = fetch_url_bytes(&repo_url).await
        && let Ok((spec_version, decomp_repo)) = decompress_icechunk_file(&raw_repo_bytes)
        && let Ok(repo_info) = RepoInfo::from_buffer(decomp_repo)
    {
        if let Ok(branches) = repo_info.branches() {
            for (b_name, snap_id) in branches {
                if b_name == "main" || b_name == "master" {
                    resolved_snap_id = Some((snap_id.to_string(), spec_version));
                    break;
                }
                if resolved_snap_id.is_none() {
                    resolved_snap_id = Some((snap_id.to_string(), spec_version));
                }
            }
        }
        if resolved_snap_id.is_none()
            && let Ok(mut tags) = repo_info.tags()
            && let Some((_tag_name, snap_id)) = tags.next()
        {
            resolved_snap_id = Some((snap_id.to_string(), spec_version));
        }
    }

    // 2. Fallback to Icechunk V1 discovery via /refs/branch.main/ref.json
    if resolved_snap_id.is_none() {
        let mut ref_bytes = fetch_url_bytes(&format!("{clean_url}/refs/branch.main/ref.json"))
            .await
            .ok();
        if ref_bytes.is_none() {
            ref_bytes = fetch_url_bytes(&format!("{clean_url}/refs/branch.master/ref.json"))
                .await
                .ok();
        }
        if ref_bytes.is_none() {
            ref_bytes = fetch_url_bytes(&format!("{clean_url}/refs/tag.latest/ref.json"))
                .await
                .ok();
        }

        if let Some(bytes) = ref_bytes {
            let text = String::from_utf8_lossy(&bytes).trim().to_string();
            let snap_id = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                val.get("snapshot")
                    .and_then(|v| v.as_str())
                    .or_else(|| val.as_str())
                    .or_else(|| val.get("id").and_then(|v| v.as_str()))
                    .map(|s| s.to_string())
                    .unwrap_or(text)
            } else {
                text.trim_matches('"').to_string()
            };
            resolved_snap_id = Some((snap_id, SpecVersionBin::V1));
        }
    }

    let (snap_id, spec_version) = resolved_snap_id.ok_or_else(|| {
        format!(
            "Failed to find Icechunk branch reference at '{clean_url}/repo' (V2) or '{clean_url}/refs/branch.main/ref.json' (V1). Ensure the server allows CORS."
        )
    })?;

    log::info!("[WASM Icechunk] Resolved snapshot ID: {snap_id} (spec: {spec_version:?})");

    // 3. Fetch snapshot file
    let snap_url = format!("{clean_url}/snapshots/{snap_id}");
    let raw_snap_bytes = fetch_url_bytes(&snap_url)
        .await
        .map_err(|e| format!("Failed fetching snapshot from '{snap_url}': {e}"))?;

    let (snap_spec_version, decomp_snap) = decompress_icechunk_file(&raw_snap_bytes)
        .map_err(|e| format!("Failed decompressing snapshot: {e}"))?;

    let actual_version =
        if snap_spec_version == SpecVersionBin::V1 && spec_version != SpecVersionBin::V1 {
            spec_version
        } else {
            snap_spec_version
        };

    let snapshot = Snapshot::from_buffer(actual_version, decomp_snap)
        .map_err(|e| format!("Failed parsing Icechunk snapshot FlatBuffers: {e:?}"))?;

    let mut variables = Vec::new();
    let mut manifest_map = HashMap::new();

    // 4. Process nodes in snapshot
    for node_res in snapshot.iter() {
        let node = node_res.map_err(|e| format!("Failed iterating snapshot nodes: {e:?}"))?;
        let node_path = node.path.to_string();
        let clean_path = node_path.trim_matches('/');

        // Store Zarr metadata in inner memory store
        if !node.user_data.is_empty() {
            let meta_key = if clean_path.is_empty() {
                "zarr.json".to_string()
            } else {
                format!("{clean_path}/zarr.json")
            };
            let _ = store
                .inner
                .insert_key_bytes(&meta_key, node.user_data.as_ref());
        }

        match node.node_data {
            NodeData::Array {
                shape,
                dimension_names,
                manifests,
            } => {
                let var_name = if clean_path.is_empty() {
                    "data".to_string()
                } else {
                    clean_path.to_string()
                };

                manifest_map.insert(
                    var_name.clone(),
                    ArrayManifestInfo {
                        node_id: node.id,
                        manifests,
                    },
                );

                let dim_names_vec: Vec<String> = dimension_names
                    .map(|dims| {
                        dims.into_iter()
                            .map(|d| format!("{d:?}").trim_matches('"').to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                let shape_u64: Vec<u64> = shape
                    .iter()
                    .map(|d| format!("{d:?}").parse().unwrap_or(0))
                    .collect();

                // Parse VariableInfo from Zarr metadata or attributes
                let mut var_info_opt = None;
                if let Ok(arr) = open_or_instantiate_array_normalized(
                    store.inner.memory_store.clone(),
                    &format!("/{clean_path}"),
                ) {
                    var_info_opt = variable_info_from_array(&arr, &var_name);
                }

                if let Some(mut var_info) = var_info_opt {
                    if !dim_names_vec.is_empty() && var_info.dimension_names.is_empty() {
                        var_info.dimension_names = dim_names_vec;
                    }
                    variables.push(var_info);
                } else {
                    let data_type = "float32".to_string();
                    let file_size = calculate_variable_size_bytes(&shape_u64, &data_type);

                    variables.push(VariableInfo {
                        name: var_name,
                        data_type,
                        shape: shape_u64.clone(),
                        dimension_names: dim_names_vec,
                        chunk_shape: shape_u64,
                        file_size,
                        units: None,
                        long_name: None,
                        time_coverage_start: None,
                        time_coverage_end: None,
                        temporal_resolution: None,
                        attributes: HashMap::new(),
                    });
                }
            }
            NodeData::Group => {}
        }
    }

    *store
        .array_manifests
        .write()
        .unwrap_or_else(|p| p.into_inner()) = manifest_map;

    // 5. Preload 1D coordinate arrays to populate dimension_coordinates
    let coord_candidates: Vec<String> = variables
        .iter()
        .filter(|v| v.shape.len() == 1 && v.shape.first().copied().unwrap_or(0) <= 10000)
        .map(|v| v.name.clone())
        .collect();

    for coord_name in &coord_candidates {
        if let Some(var_info) = variables.iter().find(|v| &v.name == coord_name) {
            let count = var_info.shape.first().copied().unwrap_or(0);
            if count > 0 {
                let subset = ArraySubset::new_with_shape(vec![count]);
                let _ = store
                    .preload_chunks_for_subset(coord_name, &subset, None)
                    .await;
            }
        }
    }

    let dimension_coordinates =
        crate::data::backends::coord_bounds::fetch_all_dimension_coordinates_for_variables(
            store.inner.memory_store.clone(),
            &variables,
            Some(clean_url),
        );

    let dataset_name = clean_url
        .split('/')
        .next_back()
        .unwrap_or(clean_url)
        .to_string();

    let dataset_metadata = DatasetMetadata {
        name: dataset_name,
        store_type: "icechunk".to_string(),
        variables,
        dimension_coordinates,
    };

    *store
        .inner
        .metadata
        .write()
        .unwrap_or_else(|p| p.into_inner()) = Some(dataset_metadata.clone());

    Ok(dataset_metadata)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn inspect_wasm_remote_icechunk(url: &str) -> Result<DatasetMetadata, String> {
    let clean_url = url.trim_start_matches("icechunk+").trim_end_matches('/');
    let store = WasmIcechunkBlockStore::get_or_create(clean_url);
    store.inner.inspect().map_err(|e| e.to_string())
}

/// Asynchronously loads one block on WASM for an Icechunk dataset.
#[cfg(target_arch = "wasm32")]
pub async fn load_one_icechunk_wasm_with_progress(
    request: &BlockRequest,
    on_progress: ProgressCallback<'_>,
) -> Result<OctantBlock, BlockStoreError> {
    let source_uri = &request.store.source().uri;
    let clean_url = source_uri
        .trim_start_matches("icechunk+")
        .trim_end_matches('/');
    let store = WasmIcechunkBlockStore::get_or_create(clean_url);

    let shape = request
        .store
        .inspect()
        .map(|meta| {
            meta.variables
                .iter()
                .find(|v| v.name == request.slice.variable)
                .map(|v| v.shape.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    let rank = request.slice.selections.len();
    let mut ranges = Vec::with_capacity(rank);

    for (i, sel) in request.slice.selections.iter().enumerate() {
        let dim_len = shape.get(i).copied().unwrap_or(1000) as usize;
        let (start, end) = match *sel {
            crate::data::slice_request::DimensionSelection::Index(idx) => {
                (idx, idx.saturating_add(1))
            }
            crate::data::slice_request::DimensionSelection::Range { start, end } => (start, end),
        };
        let start = start.min(dim_len.saturating_sub(1));
        let end = end.max(start + 1).min(dim_len);
        ranges.push(start as u64..end as u64);
    }

    let subset = ArraySubset::new_with_ranges(&ranges);
    log::info!(
        "[WASM Icechunk] Preloading chunks for '{}', subset: {:?}",
        request.slice.variable,
        subset.to_ranges()
    );

    // Asynchronously download any missing chunks for this slice via manifests
    if let Err(e) = store
        .preload_chunks_for_subset(&request.slice.variable, &subset, on_progress)
        .await
    {
        log::error!(
            "[WASM Icechunk] Failed downloading chunks for '{}': {e}",
            request.slice.variable
        );
        return Err(e);
    }

    // Decode slice synchronously from in-memory chunks
    match store.fetch_block_with_progress(&request.slice, None) {
        Ok(block) => {
            log::info!(
                "[WASM Icechunk] Successfully decoded block for '{}' ({} values, shape: {:?})",
                request.slice.variable,
                block.values.len(),
                block.shape
            );
            Ok(block)
        }
        Err(e) => {
            log::error!(
                "[WASM Icechunk] Failed decoding block for '{}': {e}",
                request.slice.variable
            );
            Err(e)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn load_one_icechunk_wasm_with_progress(
    request: &BlockRequest,
    on_progress: ProgressCallback<'_>,
) -> Result<OctantBlock, BlockStoreError> {
    request
        .store
        .fetch_with_progress(&request.slice, on_progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s3_to_https_conversions() {
        assert_eq!(
            s3_to_https("s3://my-bucket/path/to/chunk"),
            "https://my-bucket.s3.amazonaws.com/path/to/chunk"
        );
        assert_eq!(
            s3_to_https("s3://simple-bucket"),
            "https://simple-bucket.s3.amazonaws.com"
        );
        assert_eq!(
            s3_to_https("vcc://container-bucket/virtual/key.nc"),
            "https://container-bucket.s3.amazonaws.com/virtual/key.nc"
        );
        assert_eq!(
            s3_to_https("https://direct.domain.com/data.zarr"),
            "https://direct.domain.com/data.zarr"
        );
    }

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
}
