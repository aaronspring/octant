//! Inspection routines converting TIFF/GeoTIFF IFDs into Octant `DatasetMetadata`.

use std::collections::HashMap;

use async_tiff::{ImageFileDirectory, TIFF};

use crate::data::metadata::{DatasetMetadata, VariableInfo};

use super::coords::GeoSpatialBounds;

/// Build `DatasetMetadata` from parsed `TIFF` structure.
pub fn inspect_tiff(tiff: &TIFF, source_name: &str) -> DatasetMetadata {
    let mut variables = Vec::new();
    let mut dimension_coordinates = HashMap::new();

    for (ifd_idx, ifd) in tiff.ifds().iter().enumerate() {
        let prefix = if ifd_idx == 0 {
            String::new()
        } else {
            format!("overview_{ifd_idx}/")
        };

        let width = ifd.image_width() as u64;
        let height = ifd.image_height() as u64;
        let samples = ifd.samples_per_pixel() as u64;
        let data_type_str = extract_data_type_name(ifd);
        let attributes = extract_ifd_attributes(ifd);
        let chunk_shape = extract_chunk_shape(ifd, width, height);

        let geo_bounds = GeoSpatialBounds::from_ifd(ifd);

        if samples > 1 {
            // Multi-band 3D volume/tensor representation
            let var_name = format!("{prefix}raster");
            geo_bounds.populate_dimension_coordinates(&var_name, &mut dimension_coordinates);

            let mut chunk_3d = Vec::with_capacity(3);
            chunk_3d.push(1);
            chunk_3d.extend_from_slice(&chunk_shape);

            variables.push(VariableInfo {
                name: var_name,
                data_type: data_type_str.clone(),
                shape: vec![samples, height, width],
                dimension_names: vec!["band".to_string(), "y".to_string(), "x".to_string()],
                chunk_shape: chunk_3d,
                file_size: width
                    .saturating_mul(height)
                    .saturating_mul(samples)
                    .saturating_mul(4),
                units: None,
                long_name: Some("Multi-band raster".to_string()),
                time_coverage_start: None,
                time_coverage_end: None,
                temporal_resolution: None,
                attributes: attributes.clone(),
            });
        }

        // Individual band variables
        for band_idx in 0..samples {
            let var_name = if samples == 1 {
                if prefix.is_empty() {
                    "band_1".to_string()
                } else {
                    format!("{prefix}band_1")
                }
            } else {
                format!("{prefix}band_{}", band_idx + 1)
            };

            geo_bounds.populate_dimension_coordinates(&var_name, &mut dimension_coordinates);

            variables.push(VariableInfo {
                name: var_name,
                data_type: data_type_str.clone(),
                shape: vec![height, width],
                dimension_names: vec!["y".to_string(), "x".to_string()],
                chunk_shape: chunk_shape.clone(),
                file_size: width.saturating_mul(height).saturating_mul(4),
                units: None,
                long_name: Some(format!("Band {}", band_idx + 1)),
                time_coverage_start: None,
                time_coverage_end: None,
                temporal_resolution: None,
                attributes: attributes.clone(),
            });
        }
    }

    DatasetMetadata {
        name: source_name.to_string(),
        store_type: "GeoTIFF".to_string(),
        variables,
        dimension_coordinates,
    }
}

fn extract_data_type_name(ifd: &ImageFileDirectory) -> String {
    let dt = async_tiff::DataType::from_tags(ifd.sample_format(), ifd.bits_per_sample());
    match dt {
        Some(async_tiff::DataType::UInt8) => "uint8",
        Some(async_tiff::DataType::UInt16) => "uint16",
        Some(async_tiff::DataType::UInt32) => "uint32",
        Some(async_tiff::DataType::UInt64) => "uint64",
        Some(async_tiff::DataType::Int8) => "int8",
        Some(async_tiff::DataType::Int16) => "int16",
        Some(async_tiff::DataType::Int32) => "int32",
        Some(async_tiff::DataType::Int64) => "int64",
        Some(async_tiff::DataType::Float32) => "float32",
        Some(async_tiff::DataType::Float64) => "float64",
        Some(async_tiff::DataType::Bool) => "bool",
        None => "float32",
    }
    .to_string()
}

fn extract_chunk_shape(ifd: &ImageFileDirectory, width: u64, height: u64) -> Vec<u64> {
    if let (Some(tw), Some(th)) = (ifd.tile_width(), ifd.tile_height()) {
        vec![th as u64, tw as u64]
    } else if let Some(rps) = ifd.rows_per_strip() {
        vec![(rps as u64).min(height), width]
    } else {
        vec![height.min(256), width.min(256)]
    }
}

fn extract_ifd_attributes(ifd: &ImageFileDirectory) -> HashMap<String, String> {
    let mut attrs = HashMap::new();

    if let Some(nodata) = ifd.gdal_nodata() {
        attrs.insert("_FillValue".to_string(), nodata.to_string());
        attrs.insert("nodata".to_string(), nodata.to_string());
    }
    if let Some(desc) = ifd.image_description() {
        attrs.insert("description".to_string(), desc.to_string());
    }
    if let Some(soft) = ifd.software() {
        attrs.insert("software".to_string(), soft.to_string());
    }
    if let Some(dt) = ifd.date_time() {
        attrs.insert("datetime".to_string(), dt.to_string());
    }
    if let Some(geo) = ifd.geo_key_directory() {
        if let Some(ref cit) = geo.citation {
            attrs.insert("crs_citation".to_string(), cit.clone());
        }
        if let Some(ref proj_cit) = geo.proj_citation {
            attrs.insert("projection_citation".to_string(), proj_cit.clone());
        }
    }

    attrs
}
