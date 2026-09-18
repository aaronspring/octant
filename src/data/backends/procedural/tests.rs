//! Procedural block store integration tests.

use crate::data::backends::ProceduralBlockStore;
use crate::data::blocks::BlockStore;
use crate::data::coordinates::CoordinateGrid;
use crate::data::slice_request::{DimensionSelection, SliceRequest};

#[test]
fn test_procedural_store_metadata_bounds() {
    let store = ProceduralBlockStore::open("procedural://test").expect("open store");
    let meta = store.inspect().expect("inspect store");

    // Stretched regional bounds in metadata
    let stretched_lon = meta.get_coord_bounds_for_var(Some("stretched_regional_2d"), "lon");
    assert!(stretched_lon.is_some());
    let (min_lon, max_lon) = stretched_lon.unwrap();
    assert!((min_lon - 10.0).abs() < 1e-2);
    assert!((max_lon - 50.0).abs() < 1e-2);

    let stretched_lat = meta.get_coord_bounds_for_var(Some("stretched_regional_2d"), "lat");
    assert!(stretched_lat.is_some());
    let (min_lat, max_lat) = stretched_lat.unwrap();
    assert!((min_lat - 30.0).abs() < 1e-2);
    assert!((max_lat - 60.0).abs() < 1e-2);

    // Stepped resolution bounds in metadata
    let stepped_lon = meta.get_coord_bounds_for_var(Some("stepped_resolution_2d"), "lon");
    assert!(stepped_lon.is_some());
    let (s_min_lon, s_max_lon) = stepped_lon.unwrap();
    assert!((s_min_lon - (-40.0)).abs() < 1e-2);
    assert!((s_max_lon - 40.0).abs() < 1e-2);

    let stepped_lat = meta.get_coord_bounds_for_var(Some("stepped_resolution_2d"), "lat");
    assert!(stepped_lat.is_some());
    let (s_min_lat, s_max_lat) = stepped_lat.unwrap();
    assert!((s_min_lat - (-20.0)).abs() < 1e-2);
    assert!((s_max_lat - 20.0).abs() < 1e-2);
}

#[test]
fn test_procedural_stretched_regional_slicing_and_grid_detection() {
    let store = ProceduralBlockStore::open("procedural://test").expect("open store");
    let mut request = SliceRequest::full_range("stretched_regional_2d", &[32, 48]);
    // Sub-slice: lat 10..20, lon 12..36
    request.selections = vec![
        DimensionSelection::range(10, 20),
        DimensionSelection::range(12, 36),
    ];

    let block = store.fetch_block(&request).expect("fetch block");
    assert_eq!(block.shape, vec![10, 24]);

    let matrix = block
        .slice_2d(1, 0, &[0, 0], 1, "test", true)
        .expect("slice 2d");
    assert_eq!(matrix.width, 24);
    assert_eq!(matrix.height, 10);

    match &matrix.grid {
        CoordinateGrid::Irregular1D {
            coords_x,
            coords_y,
            lon_bounds,
            lat_bounds,
        } => {
            assert_eq!(coords_x.len(), 24);
            assert_eq!(coords_y.len(), 10);
            assert!(lon_bounds.0 >= 10.0 && lon_bounds.1 <= 50.0);
            assert!(lat_bounds.0 >= 30.0 && lat_bounds.1 <= 60.0);
        }
        other => {
            panic!("Expected Irregular1D grid for sliced stretched regional, got {other:?}")
        }
    }
}

#[test]
fn test_procedural_stepped_resolution_slicing_and_grid_detection() {
    let store = ProceduralBlockStore::open("procedural://test").expect("open store");

    // 1. Across-jump sub-slice: lat 0..16, lon 16..48 (spans across the 5x resolution jump at index 32)
    let mut request_jump = SliceRequest::full_range("stepped_resolution_2d", &[32, 64]);
    request_jump.selections = vec![
        DimensionSelection::range(0, 16),
        DimensionSelection::range(16, 48),
    ];

    let block_jump = store.fetch_block(&request_jump).expect("fetch block");
    assert_eq!(block_jump.shape, vec![16, 32]);

    let matrix_jump = block_jump
        .slice_2d(1, 0, &[0, 0], 1, "test", true)
        .expect("slice 2d");
    assert_eq!(matrix_jump.width, 32);
    assert_eq!(matrix_jump.height, 16);

    match &matrix_jump.grid {
        CoordinateGrid::Irregular1D {
            coords_x,
            coords_y,
            lon_bounds,
            lat_bounds,
        } => {
            assert_eq!(coords_x.len(), 32);
            assert_eq!(coords_y.len(), 16);
            assert!(lon_bounds.0 < 0.0 && lon_bounds.1 > 0.0);
            assert!((lat_bounds.1 - 20.0).abs() < 1e-2);
        }
        other => {
            panic!("Expected Irregular1D grid for across-jump stepped resolution, got {other:?}")
        }
    }

    // 2. Uniform sub-slice: lat 0..16, lon 0..32 (entirely within fine uniform left half)
    let mut request_uniform = SliceRequest::full_range("stepped_resolution_2d", &[32, 64]);
    request_uniform.selections = vec![
        DimensionSelection::range(0, 16),
        DimensionSelection::range(0, 32),
    ];

    let block_uniform = store.fetch_block(&request_uniform).expect("fetch block");
    let matrix_uniform = block_uniform
        .slice_2d(1, 0, &[0, 0], 1, "test", true)
        .expect("slice 2d");

    match &matrix_uniform.grid {
        CoordinateGrid::RegionalRegular {
            lon_bounds,
            lat_bounds,
        } => {
            assert!((lon_bounds.0 - (-40.0)).abs() < 1e-2);
            assert!((lat_bounds.1 - 20.0).abs() < 1e-2);
        }
        other => panic!("Expected RegionalRegular grid for uniform half slice, got {other:?}"),
    }
}

#[test]
fn test_procedural_gaussian_wave_packet_4d_slicing() {
    let store = ProceduralBlockStore::open("procedural://volume").expect("open store");
    let mut request = SliceRequest::full_range("gaussian_wave_packet_4d", &[20, 32, 32, 32]);
    request.selections = vec![
        DimensionSelection::index(2),
        DimensionSelection::index(5),
        DimensionSelection::range(4, 20),
        DimensionSelection::range(8, 24),
    ];

    let block = store.fetch_block(&request).expect("fetch block");
    assert_eq!(block.shape, vec![1, 1, 16, 16]);

    let matrix = block
        .slice_2d(3, 2, &[0, 0, 0, 0], 1, "test", true)
        .expect("slice 2d");
    assert_eq!(matrix.width, 16);
    assert_eq!(matrix.height, 16);

    match &matrix.grid {
        CoordinateGrid::RegionalRegular {
            lon_bounds,
            lat_bounds,
        } => {
            assert!(lon_bounds.0 > -180.0 && lon_bounds.1 < 180.0);
            assert!(lat_bounds.0 > -90.0 && lat_bounds.1 < 90.0);
        }
        other => panic!("Expected RegionalRegular grid for sliced 4d packet, got {other:?}"),
    }
}
