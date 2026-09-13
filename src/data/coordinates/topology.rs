//! Polymorphic strategy trait representing a spatial grid structure.

use crate::plots::PlotType;

/// Polymorphic strategy representing a spatial grid structure on a plane or spherical manifold.
pub trait GridTopology: Send + Sync + std::fmt::Debug {
    /// Human-readable name of the grid topology (e.g. "HEALPix (Ring)", "Curvilinear 2D", "Cartesian Regular").
    fn name(&self) -> &'static str;

    /// Number of tensor dimensions used for spatial representation (e.g. 2 for Cartesian/Curvilinear, 1 for HEALPix/ICON).
    fn spatial_rank(&self) -> usize;

    /// Supported visualization modes for this topology.
    fn supported_plot_types(&self) -> &'static [PlotType];

    /// Aspect ratio of the spatial domain (e.g., width/height for Cartesian, 2.0 for global cylindrical/HEALPix).
    fn data_aspect_ratio(&self, width: usize, height: usize) -> f32;

    /// Maps continuous surface/mesh UV [0, 1] x [0, 1] to discrete cell indices (px, py).
    fn surface_uv_to_cell(&self, u: f32, v: f32, width: usize, height: usize) -> (usize, usize);

    /// Maps normalized 2D viewport coordinates [0, 1] x [0, 1] to discrete cell indices (px, py).
    fn norm_to_cell(
        &self,
        norm_x: f32,
        norm_y: f32,
        width: usize,
        height: usize,
    ) -> (usize, usize) {
        let w = width.max(1);
        let h = height.max(1);
        let px = ((norm_x.clamp(0.0, 1.0) * w as f32).floor() as usize).min(w.saturating_sub(1));
        let py = ((norm_y.clamp(0.0, 1.0) * h as f32).floor() as usize).min(h.saturating_sub(1));
        (px, py)
    }

    /// Maps continuous spherical (lon_rad, lat_rad) to nearest discrete cell indices (px, py).
    fn lon_lat_to_cell(
        &self,
        lon_rad: f32,
        lat_rad: f32,
        width: usize,
        height: usize,
    ) -> Option<(usize, usize)>;

    /// Computes normalized [0, 1] viewport coordinates (u_c, v_c) for the center of cell (px, py).
    fn cell_center_norm(&self, px: usize, py: usize, width: usize, height: usize) -> (f32, f32);

    /// Computes continuous cell center (world_x, world_z) in surface plot space [-aspect, aspect] x [-1, 1].
    fn cell_center_surface_xz(
        &self,
        px: usize,
        py: usize,
        width: usize,
        height: usize,
        aspect: f32,
    ) -> (f32, f32);

    /// Computes spherical coordinates (lon_rad, lat_rad) of the discrete cell center.
    fn cell_center_lon_lat_rad(
        &self,
        px: usize,
        py: usize,
        width: usize,
        height: usize,
    ) -> (f32, f32);

    /// Integer uniform `coord_mode` passed to GPU shaders.
    fn shader_coord_mode(&self) -> u32;

    /// Returns `true` if this grid spans the full global extent (~360° lon, ~180° lat).
    fn is_global(&self) -> bool;

    /// Returns `true` if longitude bounds span >=350° and latitude spans >=160°.
    fn is_global_extent(&self) -> bool;

    /// Returns the longitude bounds [lon_min, lon_max] in degrees.
    fn lon_bounds_deg(&self) -> (f32, f32);

    /// Returns the latitude bounds [lat_min, lat_max] in degrees.
    fn lat_bounds_deg(&self) -> (f32, f32);

    /// Returns the longitude bounds [lon_min, lon_max] in radians.
    fn lon_bounds_rad(&self) -> [f32; 2] {
        let (lon_min, lon_max) = self.lon_bounds_deg();
        [lon_min.to_radians(), lon_max.to_radians()]
    }

    /// Returns the latitude bounds [lat_min, lat_max] in radians clamped to [-90°, 90°].
    fn lat_bounds_rad(&self) -> [f32; 2] {
        let (lat_min, lat_max) = self.lat_bounds_deg();
        [
            lat_min.clamp(-90.0, 90.0).to_radians(),
            lat_max.clamp(-90.0, 90.0).to_radians(),
        ]
    }

    /// Returns `true` if the grid requires non-standard geographic coordinates processing.
    fn requires_geo_coords(&self) -> bool {
        true
    }
}
