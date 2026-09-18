//! Coordinate extraction for NetCDF sliced hyperslabs.

use std::collections::HashMap;

use netcdf::{Extent, Extents};

use super::slice::read_variable_hyperslab_as_f32;

/// Extracts coordinate vectors for the sliced block dimensions from open NetCDF file.
pub fn extract_sliced_coordinates(
    file: &netcdf::File,
    dim_names: &[String],
    origin: &[usize],
    block_shape: &[usize],
) -> HashMap<String, Vec<f64>> {
    let mut coordinates: HashMap<String, Vec<f64>> = HashMap::new();
    for (i, name) in dim_names.iter().enumerate() {
        let clean = name.trim().to_lowercase();
        if let Some(coord_var) = file.variable(name).or_else(|| file.variable(&clean))
            && coord_var.dimensions().len() == 1
        {
            let dim_len = coord_var.dimensions()[0].len();
            let (start, end) = (origin[i], origin[i] + block_shape[i]);
            let start = start.min(dim_len.saturating_sub(1));
            let end = end.min(dim_len);
            let count = end.saturating_sub(start).max(1);

            let coord_extents = Extents::from(vec![Extent::SliceCount {
                start,
                count,
                stride: 1,
            }]);

            if let Ok(vals) = read_variable_hyperslab_as_f32(&coord_var, &coord_extents)
                && !vals.is_empty()
            {
                let coord_vec: Vec<f64> = vals.iter().map(|&v| v as f64).collect();
                coordinates.insert(name.clone(), coord_vec.clone());
                if clean != *name {
                    coordinates.insert(clean, coord_vec);
                }
            }
        }
    }
    coordinates
}
