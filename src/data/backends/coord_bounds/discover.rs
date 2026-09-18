//! Array discovery heuristics across group hierarchies and consolidated metadata.

use zarrs::array::Array;
use zarrs::storage::ReadableWritableListableStorage;

/// Finds and opens an array matching any candidate path across root, group children, and consolidated metadata.
pub fn discover_coord_array(
    store: ReadableWritableListableStorage,
    candidates: &[String],
    aliases: &[&str],
) -> Option<Array<dyn zarrs::storage::ReadableStorageTraits>> {
    let readable_store: zarrs::storage::ReadableStorage = store.clone();
    for clean_key in candidates {
        let clean_path = format!("/{}", clean_key);

        if let Ok(arr) = crate::utils::metadata::open_or_instantiate_array_normalized(
            readable_store.clone(),
            &clean_path,
        ) {
            return Some(arr);
        }
    }

    // Discover child group nodes from root if still not found
    if let Ok(root_path) = zarrs::node::NodePath::new("/")
        && let Ok(children) = zarrs::node::get_child_nodes(&store, &root_path, false)
    {
        for child in children {
            let child_group = child.path().as_str().trim_start_matches('/');
            if child_group.is_empty() {
                continue;
            }
            let child_target = format!("/{}", child_group);
            for clean_key in candidates {
                if let Ok(arr) = Array::open(
                    readable_store.clone(),
                    &format!("{}/{}", child_target, clean_key),
                ) {
                    return Some(arr);
                }
            }
            for alias in aliases {
                if let Ok(arr) = Array::open(
                    readable_store.clone(),
                    &format!("{}/{}", child_target, alias),
                ) {
                    return Some(arr);
                }
            }
        }
    }

    if let Ok(group) = zarrs::group::Group::open(readable_store.clone(), "/")
        && let Some(zarrs::metadata_ext::group::consolidated_metadata::ConsolidatedMetadata {
            metadata,
            ..
        }) = group.consolidated_metadata()
    {
        for path in candidates {
            let key = path.trim_start_matches('/');
            if let Some(node_meta) = metadata.get(key).or_else(|| metadata.get(path))
                && let Some(arr) = crate::utils::metadata::instantiate_array_from_node_metadata(
                    readable_store.clone(),
                    path,
                    node_meta,
                )
            {
                return Some(arr);
            }
        }
    }

    None
}
