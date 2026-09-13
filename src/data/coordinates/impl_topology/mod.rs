//! GridTopology implementation for CoordinateGrid enum.

mod cells;

use super::healpix::HealpixOrder;
use super::topologies::{
    CartesianTopology, CurvilinearTopology, HealpixTopology, Irregular1DTopology,
};
use super::topology::GridTopology;
use super::types::CoordinateGrid;
use crate::plots::PlotType;

impl GridTopology for CoordinateGrid {
    fn name(&self) -> &'static str {
        match self {
            Self::GlobalRegular => "Cartesian Regular (Global)",
            Self::RegionalRegular { .. } => "Cartesian Regular (Regional)",
            Self::Irregular1D { .. } => "Rectilinear 1D (Irregular)",
            Self::Curvilinear2D { .. } => "Curvilinear 2D",
            Self::Healpix {
                ordering: HealpixOrder::Ring,
                ..
            } => "HEALPix (Ring)",
            Self::Healpix {
                ordering: HealpixOrder::Nested,
                ..
            } => "HEALPix (Nested)",
        }
    }

    fn spatial_rank(&self) -> usize {
        if matches!(self, Self::Healpix { .. }) {
            1
        } else {
            2
        }
    }

    fn supported_plot_types(&self) -> &'static [PlotType] {
        match self {
            Self::Healpix { .. } => HealpixTopology::default_supported_plots(),
            Self::GlobalRegular | Self::RegionalRegular { .. } => {
                CartesianTopology::default_supported_plots()
            }
            Self::Irregular1D { .. } => Irregular1DTopology::default_supported_plots(),
            Self::Curvilinear2D { .. } => CurvilinearTopology::default_supported_plots(),
        }
    }

    fn data_aspect_ratio(&self, width: usize, height: usize) -> f32 {
        match self {
            Self::GlobalRegular | Self::Healpix { .. } => 2.0,
            _ => (width.max(1) as f32 / height.max(1) as f32).clamp(0.1, 10.0),
        }
    }

    fn surface_uv_to_cell(&self, u: f32, v: f32, width: usize, height: usize) -> (usize, usize) {
        cells::surface_uv_to_cell(self, u, v, width, height)
    }

    fn norm_to_cell(
        &self,
        norm_x: f32,
        norm_y: f32,
        width: usize,
        height: usize,
    ) -> (usize, usize) {
        cells::norm_to_cell(self, norm_x, norm_y, width, height)
    }

    fn lon_lat_to_cell(
        &self,
        lon_rad: f32,
        lat_rad: f32,
        width: usize,
        height: usize,
    ) -> Option<(usize, usize)> {
        cells::lon_lat_to_cell(self, lon_rad, lat_rad, width, height)
    }

    fn cell_center_norm(&self, px: usize, py: usize, width: usize, height: usize) -> (f32, f32) {
        cells::cell_center_norm(self, px, py, width, height)
    }

    fn cell_center_surface_xz(
        &self,
        px: usize,
        py: usize,
        width: usize,
        height: usize,
        aspect: f32,
    ) -> (f32, f32) {
        cells::cell_center_surface_xz(self, px, py, width, height, aspect)
    }

    fn cell_center_lon_lat_rad(
        &self,
        px: usize,
        py: usize,
        width: usize,
        height: usize,
    ) -> (f32, f32) {
        cells::cell_center_lon_lat_rad(self, px, py, width, height)
    }

    fn shader_coord_mode(&self) -> u32 {
        match self {
            Self::GlobalRegular => 0,
            Self::RegionalRegular { .. } => 1,
            Self::Irregular1D { .. } => 2,
            Self::Curvilinear2D { .. } => 3,
            Self::Healpix {
                ordering: HealpixOrder::Ring,
                ..
            } => 4,
            Self::Healpix {
                ordering: HealpixOrder::Nested,
                ..
            } => 5,
        }
    }

    fn is_global(&self) -> bool {
        match self {
            Self::GlobalRegular | Self::Healpix { .. } => true,
            _ => self.is_global_extent(),
        }
    }

    fn is_global_extent(&self) -> bool {
        let (lon_min, lon_max) = self.lon_bounds_deg();
        let (lat_min, lat_max) = self.lat_bounds_deg();
        (lon_max - lon_min).abs() >= 350.0 && (lat_max - lat_min).abs() >= 160.0
    }

    fn lon_bounds_deg(&self) -> (f32, f32) {
        match self {
            Self::GlobalRegular => (-180.0, 180.0),
            Self::RegionalRegular { lon_bounds, .. }
            | Self::Irregular1D { lon_bounds, .. }
            | Self::Curvilinear2D { lon_bounds, .. } => *lon_bounds,
            Self::Healpix { .. } => (0.0, 360.0),
        }
    }

    fn lat_bounds_deg(&self) -> (f32, f32) {
        match self {
            Self::GlobalRegular => (-90.0, 90.0),
            Self::RegionalRegular { lat_bounds, .. }
            | Self::Irregular1D { lat_bounds, .. }
            | Self::Curvilinear2D { lat_bounds, .. } => *lat_bounds,
            Self::Healpix { .. } => (-90.0, 90.0),
        }
    }

    fn requires_geo_coords(&self) -> bool {
        !matches!(self, Self::GlobalRegular)
    }
}
