//! Zarr storage coordinate bounds retrieval and caching.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use zarrs::array::{Array, ArraySubset};
use zarrs::storage::ReadableWritableListableStorage;

#[allow(clippy::type_complexity)]
static COORD_VALUES_CACHE: OnceLock<RwLock<HashMap<String, Option<Vec<String>>>>> = OnceLock::new();

/// Fetches all dimension coordinate values for the specified dimension names across the store.
pub fn fetch_all_dimension_coordinates(
    store: ReadableWritableListableStorage,
    dim_names: &[String],
    store_url_hint: Option<&str>,
) -> HashMap<String, Vec<String>> {
    let mut coords_map = HashMap::new();
    let url_hint = store_url_hint.unwrap_or("local");
    let total_dims = dim_names.len();

    for (i, name) in dim_names.iter().enumerate() {
        let clean = name.trim().to_lowercase();
        if coords_map.contains_key(&clean) {
            continue;
        }

        if let Some(values) =
            get_cached_coord_values_with_rank(store.clone(), url_hint, name, i, total_dims)
        {
            coords_map.insert(clean.clone(), values.clone());
            if clean != *name {
                coords_map.insert(name.clone(), values);
            }
        }
    }

    coords_map
}

/// Fetches all dimension coordinate values aware of variable group paths and hierarchy.
pub fn fetch_all_dimension_coordinates_for_variables(
    store: ReadableWritableListableStorage,
    variables: &[crate::data::VariableInfo],
    store_url_hint: Option<&str>,
) -> HashMap<String, Vec<String>> {
    let mut coords_map = HashMap::new();
    let url_hint = store_url_hint.unwrap_or("local");

    // Collect all group prefixes from variables
    let mut group_prefixes = Vec::new();
    for var in variables {
        if let Some(gp) = var.group_path() {
            let mut curr = String::new();
            for seg in gp.split('/') {
                if !curr.is_empty() {
                    curr.push('/');
                }
                curr.push_str(seg);
                if !group_prefixes.contains(&curr) {
                    group_prefixes.push(curr.clone());
                }
            }
        }
    }

    for var in variables {
        let group_path = var.group_path();
        let total_dims = var.dimension_names.len();
        for (i, name) in var.dimension_names.iter().enumerate() {
            let clean = name.trim().to_lowercase();
            if coords_map.contains_key(&clean) {
                continue;
            }

            if let Some(values) = get_cached_coord_values_scoped(
                store.clone(),
                url_hint,
                name,
                group_path,
                &group_prefixes,
                i,
                total_dims,
            ) {
                coords_map.insert(clean.clone(), values.clone());
                if clean != *name {
                    coords_map.insert(name.clone(), values.clone());
                }
                if let Some(gp) = group_path {
                    let scoped_key = format!("{}/{}", gp.trim().to_lowercase(), clean);
                    coords_map.insert(scoped_key, values);
                }
            }
        }
    }

    // Also fallback to root dim names if any remain unresolved
    for var in variables {
        for (i, name) in var.dimension_names.iter().enumerate() {
            let clean = name.trim().to_lowercase();
            if !coords_map.contains_key(&clean)
                && let Some(values) = get_cached_coord_values_scoped(
                    store.clone(),
                    url_hint,
                    name,
                    None,
                    &group_prefixes,
                    i,
                    var.dimension_names.len(),
                )
            {
                coords_map.insert(clean.clone(), values.clone());
                if clean != *name {
                    coords_map.insert(name.clone(), values);
                }
            }
        }
    }

    coords_map
}

#[allow(clippy::single_range_in_vec_init)]
pub fn get_cached_coord_bounds(
    store: ReadableWritableListableStorage,
    store_url: &str,
    dim_name: &str,
) -> Option<(f64, f64)> {
    get_cached_coord_bounds_with_rank(store, store_url, dim_name, usize::MAX, 0)
}

#[allow(clippy::single_range_in_vec_init)]
pub fn get_cached_coord_bounds_with_rank(
    store: ReadableWritableListableStorage,
    store_url: &str,
    dim_name: &str,
    dim_idx: usize,
    total_dims: usize,
) -> Option<(f64, f64)> {
    get_cached_coord_bounds_scoped(store, store_url, dim_name, None, &[], dim_idx, total_dims)
}

#[allow(clippy::single_range_in_vec_init)]
pub fn get_cached_coord_bounds_scoped(
    store: ReadableWritableListableStorage,
    store_url: &str,
    dim_name: &str,
    group_scope: Option<&str>,
    known_groups: &[String],
    dim_idx: usize,
    total_dims: usize,
) -> Option<(f64, f64)> {
    let values = get_cached_coord_values_scoped(
        store,
        store_url,
        dim_name,
        group_scope,
        known_groups,
        dim_idx,
        total_dims,
    )?;
    let first: f64 = values.first()?.parse().ok()?;
    let last: f64 = values.last()?.parse().ok()?;
    Some((first, last))
}

#[allow(clippy::single_range_in_vec_init)]
pub fn get_cached_coord_values_with_rank(
    store: ReadableWritableListableStorage,
    store_url: &str,
    dim_name: &str,
    dim_idx: usize,
    total_dims: usize,
) -> Option<Vec<String>> {
    get_cached_coord_values_scoped(store, store_url, dim_name, None, &[], dim_idx, total_dims)
}

#[allow(clippy::single_range_in_vec_init)]
pub fn get_cached_coord_values_scoped(
    store: ReadableWritableListableStorage,
    store_url: &str,
    dim_name: &str,
    group_scope: Option<&str>,
    known_groups: &[String],
    dim_idx: usize,
    total_dims: usize,
) -> Option<Vec<String>> {
    let cache_lock = COORD_VALUES_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    let clean_url = store_url.trim().to_lowercase();
    let clean_dim = dim_name.trim().to_lowercase();
    let exact_key = format!("{}:{}:{}", clean_url, group_scope.unwrap_or(""), clean_dim);

    if let Ok(cache) = cache_lock.read() {
        if let Some(values) = cache.get(&exact_key) {
            return values.clone();
        }
        let root_key = format!("{}::{}", clean_url, clean_dim);
        if let Some(values) = cache.get(&root_key) {
            return values.clone();
        }
        let url_prefix = format!("{}:", clean_url);
        let dim_suffix = format!(":{}", clean_dim);
        for (k, v) in cache.iter() {
            if k.starts_with(&url_prefix)
                && k.ends_with(&dim_suffix)
                && let Some(values) = v
            {
                return Some(values.clone());
            }
        }
    }

    let values = read_coord_values_scoped(
        store,
        dim_name,
        group_scope,
        known_groups,
        dim_idx,
        total_dims,
    );
    if let Ok(mut cache) = cache_lock.write() {
        cache.insert(exact_key, values.clone());
    }
    values
}

#[allow(clippy::single_range_in_vec_init)]
pub fn read_coord_bounds(
    store: ReadableWritableListableStorage,
    dim_name: &str,
) -> Option<(f64, f64)> {
    read_coord_bounds_with_rank(store, dim_name, usize::MAX, 0)
}

#[allow(clippy::single_range_in_vec_init)]
pub fn read_coord_bounds_with_rank(
    store: ReadableWritableListableStorage,
    dim_name: &str,
    dim_idx: usize,
    total_dims: usize,
) -> Option<(f64, f64)> {
    read_coord_bounds_scoped(store, dim_name, None, &[], dim_idx, total_dims)
}

#[allow(clippy::single_range_in_vec_init)]
pub fn read_coord_bounds_scoped(
    store: ReadableWritableListableStorage,
    dim_name: &str,
    group_scope: Option<&str>,
    known_groups: &[String],
    dim_idx: usize,
    total_dims: usize,
) -> Option<(f64, f64)> {
    let values = read_coord_values_scoped(
        store,
        dim_name,
        group_scope,
        known_groups,
        dim_idx,
        total_dims,
    )?;
    let first: f64 = values.first()?.parse().ok()?;
    let last: f64 = values.last()?.parse().ok()?;
    Some((first, last))
}

#[allow(clippy::single_range_in_vec_init)]
pub fn read_coord_values_scoped(
    store: ReadableWritableListableStorage,
    dim_name: &str,
    group_scope: Option<&str>,
    known_groups: &[String],
    _dim_idx: usize,
    _total_dims: usize,
) -> Option<Vec<String>> {
    let clean = dim_name.trim().to_lowercase();
    let mut candidates = Vec::new();
    let mut add_candidate = |p: String| {
        let clean_key = p.trim_start_matches('/').to_string();
        if !clean_key.is_empty() && !candidates.contains(&clean_key) {
            candidates.push(clean_key);
        }
    };

    // 1. Group-scoped candidate path (e.g. atmosphere/forecast/lat)
    if let Some(scope) = group_scope {
        add_candidate(format!("{}/{}", scope, clean));
    }

    // 2. Direct root candidate paths
    add_candidate(clean.clone());

    let is_lat = clean.contains("lat") || clean == "y";
    let is_lon = clean.contains("lon") || clean == "x";

    // 3. Heuristic spatial coordinate names
    let aliases: &[&str] = if is_lat {
        &["lat", "latitude", "y", "coords/lat", "coordinates/latitude"]
    } else if is_lon {
        &[
            "lon",
            "longitude",
            "x",
            "coords/lon",
            "coordinates/longitude",
        ]
    } else {
        &[]
    };

    for alias in aliases {
        if let Some(scope) = group_scope {
            add_candidate(format!("{}/{}", scope, alias));
        }
        add_candidate(alias.to_string());
    }

    // 4. Known group prefixes across the store (e.g. nested-only stores)
    for group in known_groups {
        add_candidate(format!("{}/{}", group, clean));
        for alias in aliases {
            add_candidate(format!("{}/{}", group, alias));
        }
    }

    let mut found_array = None;
    for clean_key in &candidates {
        let clean_path = format!("/{}", clean_key);

        if let Ok(arr) =
            crate::utils::metadata::open_or_instantiate_array_normalized(store.clone(), &clean_path)
        {
            found_array = Some(arr);
            break;
        }
    }

    // 5. Discover child group nodes from root if still not found
    if found_array.is_none()
        && let Ok(root_path) = zarrs::node::NodePath::new("/")
        && let Ok(children) = zarrs::node::get_child_nodes(&store, &root_path, false)
    {
        for child in children {
            let child_group = child.path().as_str().trim_start_matches('/');
            if child_group.is_empty() {
                continue;
            }
            let child_target = format!("/{}", child_group);
            if let Ok(arr) = Array::open(store.clone(), &format!("{}/{}", child_target, clean)) {
                found_array = Some(arr);
                break;
            }
            for alias in aliases {
                if let Ok(arr) = Array::open(store.clone(), &format!("{}/{}", child_target, alias))
                {
                    found_array = Some(arr);
                    break;
                }
            }
            if found_array.is_some() {
                break;
            }
        }
    }

    if found_array.is_none()
        && let Ok(group) = zarrs::group::Group::open(store.clone(), "/")
        && let Some(zarrs::metadata_ext::group::consolidated_metadata::ConsolidatedMetadata {
            metadata,
            ..
        }) = group.consolidated_metadata()
    {
        for path in &candidates {
            let key = path.trim_start_matches('/');
            if let Some(node_meta) = metadata.get(key).or_else(|| metadata.get(path))
                && let Some(arr) = crate::utils::metadata::instantiate_array_from_node_metadata(
                    store.clone(),
                    path,
                    node_meta,
                )
            {
                found_array = Some(arr);
                break;
            }
        }
    }
    let array = found_array?;

    let len = array.shape().first().copied().unwrap_or(0) as usize;
    if len == 0 {
        return None;
    }
    if len == 1 {
        let subset_0 = ArraySubset::new_with_ranges(&[0..1]);
        let val = crate::data::backends::zarr_slice::retrieve_array_subset_as_f32(
            &array, None, &subset_0,
        )
        .ok()
        .and_then(|v| v.first().copied())?;
        return Some(vec![val.to_string()]);
    }

    // Retrieve boundary coordinates (first, last) for O(1) memory and instant resolution
    let subset_start = ArraySubset::new_with_ranges(&[0..1]);
    let subset_end = ArraySubset::new_with_ranges(&[(len as u64 - 1)..len as u64]);

    let v_start = crate::data::backends::zarr_slice::retrieve_array_subset_as_f32(
        &array,
        None,
        &subset_start,
    )
    .ok()
    .and_then(|v| v.first().map(|&x| x as f64))?;

    let v_end =
        crate::data::backends::zarr_slice::retrieve_array_subset_as_f32(&array, None, &subset_end)
            .ok()
            .and_then(|v| v.first().map(|&x| x as f64))?;

    Some(vec![v_start.to_string(), v_end.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use zarrs::array::{ArrayBuilder, DataType, FillValue};
    use zarrs::storage::store::MemoryStore;

    #[test]
    fn test_fetch_and_cache_dimension_coordinates() {
        let store = Arc::new(MemoryStore::new());
        let meta = zarrs::metadata::v3::MetadataV3::new("float32");
        let float32_dt = DataType::from_metadata(&meta).unwrap();

        // Create a 1D lat array: [-90.0, 0.0, 90.0]
        let lat_builder = ArrayBuilder::new(
            vec![3],
            vec![3],
            float32_dt.clone(),
            FillValue::from(0.0f32),
        );
        let lat_array = lat_builder
            .build(store.clone(), "/lat")
            .expect("Failed building lat array");
        lat_array
            .store_metadata()
            .expect("Failed storing lat metadata");
        lat_array
            .store_array_subset(
                &ArraySubset::new_with_shape(vec![3]),
                &[-90.0f32, 0.0, 90.0],
            )
            .expect("Failed storing lat data");

        // Create a 1D lon array: [0.0, 180.0, 360.0]
        let lon_builder = ArrayBuilder::new(vec![3], vec![3], float32_dt, FillValue::from(0.0f32));
        let lon_array = lon_builder
            .build(store.clone(), "/lon")
            .expect("Failed building lon array");
        lon_array
            .store_metadata()
            .expect("Failed storing lon metadata");
        lon_array
            .store_array_subset(
                &ArraySubset::new_with_shape(vec![3]),
                &[0.0f32, 180.0, 360.0],
            )
            .expect("Failed storing lon data");

        let dim_names = vec!["lat".to_string(), "lon".to_string()];
        let coords = fetch_all_dimension_coordinates(store.clone(), &dim_names, Some("test_store"));

        assert_eq!(
            coords.get("lat"),
            Some(&vec!["-90".to_string(), "90".to_string()])
        );
        assert_eq!(
            coords.get("lon"),
            Some(&vec!["0".to_string(), "360".to_string()])
        );

        // Test bounds
        let lat_bounds = get_cached_coord_bounds(store.clone(), "test_store", "lat");
        assert_eq!(lat_bounds, Some((-90.0, 90.0)));

        let lon_bounds = get_cached_coord_bounds(store.clone(), "test_store", "lon");
        assert_eq!(lon_bounds, Some((0.0, 360.0)));
    }
}
