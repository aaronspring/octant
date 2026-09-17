//! Discrete Global Grid System (DGGS) Zarr conventions metadata model and parsing.
//!
//! Conforms to the DGGS attribute convention for Zarr:
//! <https://github.com/zarr-conventions/dggs>

use std::collections::HashMap;

use super::healpix::HealpixOrder;
use super::naming::contains_ascii_case_insensitive;

/// DGGS reference ellipsoid or sphere specification.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DggsEllipsoid {
    pub name: String,
    pub radius: Option<f64>,
    pub semi_major_axis: Option<f64>,
    pub semi_minor_axis: Option<f64>,
    pub inverse_flattening: Option<f64>,
}

/// DGGS grid configuration parsed from Zarr metadata attributes (`"dggs"` object).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DggsMetadata {
    /// Canonical lower-cased name of the DGGS (e.g. "healpix", "h3", "s2").
    pub name: String,
    /// Refinement level / depth / order as an unsigned integer, or None for variable-sized cells.
    pub refinement_level: Option<u32>,
    /// Name of the spatial dimension in the dataset array.
    pub spatial_dimension: String,
    /// Space-filling curve indexing scheme (for HEALPix: "nested", "ring", "zuniq", etc.).
    pub indexing_scheme: Option<String>,
    /// Optional name of the 1D/2D coordinate array containing cell ids.
    pub coordinate: Option<String>,
    /// Cell id compression method: "none", "compacted", "ranges".
    pub compression: Option<String>,
    /// Reference body ellipsoid or sphere.
    pub ellipsoid: Option<DggsEllipsoid>,
}

impl DggsMetadata {
    /// Attempts to parse DGGS metadata from an attributes map.
    pub fn from_attributes(attributes: &HashMap<String, String>) -> Option<Self> {
        if let Some(dggs_str) = attributes.get("dggs")
            && let Ok(meta) = serde_json::from_str::<Self>(dggs_str)
        {
            return Some(meta);
        }

        // Check if individual flattened keys exist
        if let Some(name) = attributes
            .get("dggs_name")
            .or_else(|| attributes.get("dggs:name"))
            .or_else(|| attributes.get("grid_name"))
        {
            let spatial_dimension = attributes
                .get("dggs_spatial_dimension")
                .or_else(|| attributes.get("dggs:spatial_dimension"))
                .or_else(|| attributes.get("spatial_dimension"))
                .cloned()
                .unwrap_or_else(|| "cells".to_string());

            let refinement_level = attributes
                .get("dggs_refinement_level")
                .or_else(|| attributes.get("dggs:refinement_level"))
                .or_else(|| attributes.get("healpix_zoom"))
                .or_else(|| attributes.get("healpix_level"))
                .and_then(|s| s.parse::<u32>().ok());

            let indexing_scheme = attributes
                .get("dggs_indexing_scheme")
                .or_else(|| attributes.get("dggs:indexing_scheme"))
                .or_else(|| attributes.get("indexing_scheme"))
                .or_else(|| attributes.get("healpix_order"))
                .or_else(|| attributes.get("ordering"))
                .cloned();

            let coordinate = attributes
                .get("dggs_coordinate")
                .or_else(|| attributes.get("dggs:coordinate"))
                .cloned();

            return Some(Self {
                name: name.to_ascii_lowercase(),
                refinement_level,
                spatial_dimension,
                indexing_scheme,
                coordinate,
                compression: attributes.get("compression").cloned(),
                ellipsoid: None,
            });
        }

        None
    }

    /// Attempts to parse DGGS metadata from a serde_json::Value map.
    pub fn from_json_value(val: &serde_json::Value) -> Option<Self> {
        if let Some(dggs_obj) = val.get("dggs")
            && let Ok(meta) = serde_json::from_value::<Self>(dggs_obj.clone())
        {
            return Some(meta);
        }
        None
    }

    /// Checks if this DGGS metadata describes a HEALPix discrete global grid.
    #[inline]
    pub fn is_healpix(&self) -> bool {
        self.name.eq_ignore_ascii_case("healpix")
            || contains_ascii_case_insensitive(&self.name, "healpix")
    }

    /// Resolves the HEALPix `nside` parameter from refinement level ($2^k$) or total pixel count.
    pub fn healpix_nside(&self, actual_dim_size: usize) -> Option<usize> {
        if let Some(level) = self.refinement_level
            && level <= 29
        {
            return Some(1usize << level);
        }
        super::healpix::npix_to_nside(actual_dim_size)
    }

    /// Resolves the HEALPix ordering scheme (`Nested` vs `Ring`).
    pub fn healpix_ordering(&self) -> HealpixOrder {
        if let Some(ref scheme) = self.indexing_scheme {
            if scheme.eq_ignore_ascii_case("nested")
                || scheme.eq_ignore_ascii_case("nest")
                || scheme.ends_with("uniq")
            {
                return HealpixOrder::Nested;
            }
            if scheme.eq_ignore_ascii_case("ring") {
                return HealpixOrder::Ring;
            }
        }
        HealpixOrder::Ring
    }
}
