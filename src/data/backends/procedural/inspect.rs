//! Synthetic dataset metadata generator for procedural benchmarks.

use std::collections::HashMap;

use super::healpix_meta::build_healpix_metadata;
use crate::data::blocks::BlockStoreError;
use crate::data::metadata::{DatasetMetadata, VariableInfo};
use crate::data::procedural::{
    generate_clenshaw_curtis_coords, generate_gaussian_coords, generate_stepped_resolution_coords,
    generate_stretched_regional_coords,
};

fn make_var(
    name: &str,
    shape: Vec<u64>,
    chunk: Vec<u64>,
    dims: &[&str],
    units: &str,
    long_name: &str,
    temporal: Option<&str>,
) -> VariableInfo {
    let file_size = shape.iter().product::<u64>() * 4;
    VariableInfo {
        name: name.to_string(),
        data_type: "float32".to_string(),
        shape,
        chunk_shape: chunk,
        dimension_names: dims.iter().map(|s| s.to_string()).collect(),
        units: Some(units.to_string()),
        long_name: Some(long_name.to_string()),
        temporal_resolution: temporal.map(String::from),
        time_coverage_start: None,
        time_coverage_end: None,
        file_size,
        attributes: HashMap::new(),
    }
}

pub fn inspect_procedural(uri: &str) -> Result<DatasetMetadata, BlockStoreError> {
    if uri.contains("healpix") {
        return Ok(build_healpix_metadata());
    }

    let is_4d = uri.contains("volume") || uri.contains("4d");

    let vars = if is_4d {
        vec![
            make_var(
                "gaussian_wave_packet_4d",
                vec![20, 32, 32, 32],
                vec![1, 32, 32, 32],
                &["time", "depth", "lat", "lon"],
                "K",
                "4D Known-Truth Gaussian Wave Packet (Procedural)",
                Some("1 day"),
            ),
            make_var(
                "procedural_matrix_2d",
                vec![64, 64],
                vec![64, 64],
                &["y", "x"],
                "dimensionless",
                "2D Procedural Wave Field",
                None,
            ),
        ]
    } else {
        vec![
            make_var(
                "clenshaw_curtis_2d",
                vec![64, 128],
                vec![64, 128],
                &["lat", "lon"],
                "dimensionless",
                "2D Clenshaw-Curtis Grid (Boundary Compressed)",
                None,
            ),
            make_var(
                "gaussian_grid_2d",
                vec![64, 128],
                vec![64, 128],
                &["lat", "lon"],
                "K",
                "2D Gaussian Latitude Grid (Poles Compressed)",
                None,
            ),
            make_var(
                "stretched_regional_2d",
                vec![32, 48],
                vec![32, 48],
                &["lat", "lon"],
                "dimensionless",
                "2D Geometrically Stretched Regional Grid [10E..50E, 30N..60N]",
                None,
            ),
            make_var(
                "stepped_resolution_2d",
                vec![32, 64],
                vec![32, 64],
                &["lat", "lon"],
                "dimensionless",
                "2D Stepped Multi-Resolution Grid (5x Resolution Jump)",
                None,
            ),
            make_var(
                "gaussian_wave_packet_4d",
                vec![20, 32, 32, 32],
                vec![1, 32, 32, 32],
                &["time", "depth", "lat", "lon"],
                "K",
                "4D Known-Truth Gaussian Wave Packet (Procedural)",
                Some("1 day"),
            ),
            make_var(
                "procedural_matrix_2d",
                vec![64, 64],
                vec![64, 64],
                &["y", "x"],
                "dimensionless",
                "2D Procedural Wave Field",
                None,
            ),
        ]
    };

    let mut dim_coords = HashMap::new();

    if is_4d {
        let t_coords: Vec<String> = (0..20).map(|t| format!("{t}")).collect();
        let z_coords: Vec<String> = (0..32)
            .map(|z| format!("{:.1}", z as f64 * (1000.0 / 31.0)))
            .collect();
        let lat_coords: Vec<String> = (0..32)
            .map(|j| format!("{:.3}", 90.0 - j as f64 * (180.0 / 31.0)))
            .collect();
        let lon_coords: Vec<String> = (0..32)
            .map(|i| format!("{:.3}", -180.0 + i as f64 * (360.0 / 31.0)))
            .collect();
        let xy_coords: Vec<String> = (0..64).map(|i| format!("{i}")).collect();

        dim_coords.insert("gaussian_wave_packet_4d/time".to_string(), t_coords.clone());
        dim_coords.insert(
            "gaussian_wave_packet_4d/depth".to_string(),
            z_coords.clone(),
        );
        dim_coords.insert(
            "gaussian_wave_packet_4d/lat".to_string(),
            lat_coords.clone(),
        );
        dim_coords.insert(
            "gaussian_wave_packet_4d/lon".to_string(),
            lon_coords.clone(),
        );
        dim_coords.insert("procedural_matrix_2d/y".to_string(), xy_coords.clone());
        dim_coords.insert("procedural_matrix_2d/x".to_string(), xy_coords.clone());

        dim_coords.insert("time".to_string(), t_coords);
        dim_coords.insert("depth".to_string(), z_coords);
        dim_coords.insert("lat".to_string(), lat_coords);
        dim_coords.insert("lon".to_string(), lon_coords);
        dim_coords.insert("y".to_string(), xy_coords.clone());
        dim_coords.insert("x".to_string(), xy_coords);
    } else {
        let (clenshaw_x, clenshaw_y) = generate_clenshaw_curtis_coords(128, 64);
        let (gauss_x, gauss_y) = generate_gaussian_coords(128, 64);
        let (stretched_x, stretched_y) = generate_stretched_regional_coords(48, 32);
        let (stepped_x, stepped_y) = generate_stepped_resolution_coords(64, 32);

        let clenshaw_x_str: Vec<String> = clenshaw_x.iter().map(|v| format!("{v:.3}")).collect();
        let clenshaw_y_str: Vec<String> = clenshaw_y.iter().map(|v| format!("{v:.3}")).collect();
        let gauss_x_str: Vec<String> = gauss_x.iter().map(|v| format!("{v:.3}")).collect();
        let gauss_y_str: Vec<String> = gauss_y.iter().map(|v| format!("{v:.3}")).collect();
        let stretched_x_str: Vec<String> = stretched_x.iter().map(|v| format!("{v:.3}")).collect();
        let stretched_y_str: Vec<String> = stretched_y.iter().map(|v| format!("{v:.3}")).collect();
        let stepped_x_str: Vec<String> = stepped_x.iter().map(|v| format!("{v:.3}")).collect();
        let stepped_y_str: Vec<String> = stepped_y.iter().map(|v| format!("{v:.3}")).collect();

        let t_coords: Vec<String> = (0..20).map(|t| format!("{t}")).collect();
        let z_coords: Vec<String> = (0..32)
            .map(|z| format!("{:.1}", z as f64 * (1000.0 / 31.0)))
            .collect();
        let lat_coords_32: Vec<String> = (0..32)
            .map(|j| format!("{:.3}", 90.0 - j as f64 * (180.0 / 31.0)))
            .collect();
        let lon_coords_32: Vec<String> = (0..32)
            .map(|i| format!("{:.3}", -180.0 + i as f64 * (360.0 / 31.0)))
            .collect();
        let xy_coords: Vec<String> = (0..64).map(|i| format!("{i}")).collect();

        dim_coords.insert("clenshaw_curtis_2d/lon".to_string(), clenshaw_x_str.clone());
        dim_coords.insert("clenshaw_curtis_2d/lat".to_string(), clenshaw_y_str.clone());
        dim_coords.insert("gaussian_grid_2d/lon".to_string(), gauss_x_str);
        dim_coords.insert("gaussian_grid_2d/lat".to_string(), gauss_y_str);
        dim_coords.insert("stretched_regional_2d/lon".to_string(), stretched_x_str);
        dim_coords.insert("stretched_regional_2d/lat".to_string(), stretched_y_str);
        dim_coords.insert("stepped_resolution_2d/lon".to_string(), stepped_x_str);
        dim_coords.insert("stepped_resolution_2d/lat".to_string(), stepped_y_str);
        dim_coords.insert("gaussian_wave_packet_4d/time".to_string(), t_coords.clone());
        dim_coords.insert(
            "gaussian_wave_packet_4d/depth".to_string(),
            z_coords.clone(),
        );
        dim_coords.insert("gaussian_wave_packet_4d/lat".to_string(), lat_coords_32);
        dim_coords.insert("gaussian_wave_packet_4d/lon".to_string(), lon_coords_32);
        dim_coords.insert("procedural_matrix_2d/y".to_string(), xy_coords.clone());
        dim_coords.insert("procedural_matrix_2d/x".to_string(), xy_coords.clone());

        dim_coords.insert("lon".to_string(), clenshaw_x_str);
        dim_coords.insert("lat".to_string(), clenshaw_y_str);
        dim_coords.insert("time".to_string(), t_coords);
        dim_coords.insert("depth".to_string(), z_coords);
        dim_coords.insert("y".to_string(), xy_coords.clone());
        dim_coords.insert("x".to_string(), xy_coords);
    }

    Ok(DatasetMetadata {
        name: if is_4d {
            "4D Known-Truth Procedural Store".to_string()
        } else {
            "Ground Truth Irregular & Procedural Store".to_string()
        },
        store_type: "Procedural / Ground Truth".to_string(),
        variables: vars,
        dimension_coordinates: dim_coords,
    })
}
