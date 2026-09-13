//! HEALPix discrete global grid topology.

use crate::data::coordinates::healpix::{
    HealpixOrder, ang2pix_nest, ang2pix_ring, pix2ang_nest, pix2ang_ring,
};
use crate::data::coordinates::topology::GridTopology;
use crate::plots::PlotType;
use std::sync::Arc;

static HEALPIX_PLOT_TYPES: &[PlotType] = &[
    PlotType::Heatmap,
    PlotType::Line,
    PlotType::Surface,
    PlotType::Sphere,
];

/// HEALPix (Hierarchical Equal Area isoLatitude Pixelation) discrete global grid.
#[derive(Clone, Debug, PartialEq)]
pub struct HealpixTopology {
    pub nside: usize,
    pub ordering: HealpixOrder,
    pub npix: usize,
    pub coords_lon: Option<Arc<[f32]>>,
    pub coords_lat: Option<Arc<[f32]>>,
}

impl HealpixTopology {
    #[inline]
    pub fn default_supported_plots() -> &'static [PlotType] {
        HEALPIX_PLOT_TYPES
    }

    #[inline]
    pub fn lon_lat_to_cell_calc(
        nside: usize,
        ordering: HealpixOrder,
        npix: usize,
        lon_rad: f32,
        lat_rad: f32,
    ) -> (usize, usize) {
        let px = match ordering {
            HealpixOrder::Ring => ang2pix_ring(nside, lon_rad, lat_rad),
            HealpixOrder::Nested => ang2pix_nest(nside, lon_rad, lat_rad),
        };
        (px.min(npix.saturating_sub(1)), 0)
    }

    #[inline]
    pub fn cell_center_norm_calc(nside: usize, ordering: HealpixOrder, px: usize) -> (f32, f32) {
        let (lon_rad, lat_rad) = match ordering {
            HealpixOrder::Ring => pix2ang_ring(nside, px),
            HealpixOrder::Nested => pix2ang_nest(nside, px),
        };
        let two_pi = 2.0 * std::f32::consts::PI;
        let pi = std::f32::consts::PI;
        let lon_wrapped = (lon_rad % two_pi + two_pi) % two_pi;
        let u_c = if lon_wrapped > pi {
            (lon_wrapped - two_pi) / two_pi + 0.5
        } else {
            lon_wrapped / two_pi + 0.5
        };
        let v_c = 0.5 - lat_rad / pi;
        (u_c.clamp(0.0, 1.0), v_c.clamp(0.0, 1.0))
    }

    #[inline]
    pub fn cell_center_surface_xz_calc(
        nside: usize,
        ordering: HealpixOrder,
        px: usize,
        aspect: f32,
    ) -> (f32, f32) {
        let (lon_rad, lat_rad) = match ordering {
            HealpixOrder::Ring => pix2ang_ring(nside, px),
            HealpixOrder::Nested => pix2ang_nest(nside, px),
        };
        let u_c = lon_rad / (2.0 * std::f32::consts::PI);
        let v_c = 0.5 - (lat_rad / std::f32::consts::PI);
        ((2.0 * u_c - 1.0) * aspect, 2.0 * v_c - 1.0)
    }

    #[inline]
    pub fn cell_center_lon_lat_rad_calc(
        nside: usize,
        ordering: HealpixOrder,
        coords_lon: Option<&[f32]>,
        coords_lat: Option<&[f32]>,
        px: usize,
    ) -> (f32, f32) {
        if let (Some(lons), Some(lats)) = (coords_lon, coords_lat)
            && let (Some(&lon_deg), Some(&lat_deg)) = (lons.get(px), lats.get(px))
        {
            (lon_deg.to_radians(), lat_deg.to_radians())
        } else {
            match ordering {
                HealpixOrder::Ring => pix2ang_ring(nside, px),
                HealpixOrder::Nested => pix2ang_nest(nside, px),
            }
        }
    }
}

impl GridTopology for HealpixTopology {
    fn name(&self) -> &'static str {
        match self.ordering {
            HealpixOrder::Ring => "HEALPix (Ring)",
            HealpixOrder::Nested => "HEALPix (Nested)",
        }
    }

    fn spatial_rank(&self) -> usize {
        1
    }

    fn supported_plot_types(&self) -> &'static [PlotType] {
        HEALPIX_PLOT_TYPES
    }

    fn data_aspect_ratio(&self, _width: usize, _height: usize) -> f32 {
        2.0
    }

    fn surface_uv_to_cell(&self, u: f32, v: f32, _width: usize, _height: usize) -> (usize, usize) {
        let lon_rad = u.clamp(0.0, 1.0) * 2.0 * std::f32::consts::PI;
        let lat_rad = (0.5 - v.clamp(0.0, 1.0)) * std::f32::consts::PI;
        Self::lon_lat_to_cell_calc(self.nside, self.ordering, self.npix, lon_rad, lat_rad)
    }

    fn norm_to_cell(
        &self,
        norm_x: f32,
        norm_y: f32,
        _width: usize,
        _height: usize,
    ) -> (usize, usize) {
        let lon_rad = (norm_x.clamp(0.0, 1.0) - 0.5) * 2.0 * std::f32::consts::PI;
        let lat_rad = (0.5 - norm_y.clamp(0.0, 1.0)) * std::f32::consts::PI;
        Self::lon_lat_to_cell_calc(self.nside, self.ordering, self.npix, lon_rad, lat_rad)
    }

    fn lon_lat_to_cell(
        &self,
        lon_rad: f32,
        lat_rad: f32,
        _width: usize,
        _height: usize,
    ) -> Option<(usize, usize)> {
        Some(Self::lon_lat_to_cell_calc(
            self.nside,
            self.ordering,
            self.npix,
            lon_rad,
            lat_rad,
        ))
    }

    fn cell_center_norm(&self, px: usize, _py: usize, _width: usize, _height: usize) -> (f32, f32) {
        Self::cell_center_norm_calc(self.nside, self.ordering, px)
    }

    fn cell_center_surface_xz(
        &self,
        px: usize,
        _py: usize,
        _width: usize,
        _height: usize,
        aspect: f32,
    ) -> (f32, f32) {
        Self::cell_center_surface_xz_calc(self.nside, self.ordering, px, aspect)
    }

    fn cell_center_lon_lat_rad(
        &self,
        px: usize,
        _py: usize,
        _width: usize,
        _height: usize,
    ) -> (f32, f32) {
        Self::cell_center_lon_lat_rad_calc(
            self.nside,
            self.ordering,
            self.coords_lon.as_deref(),
            self.coords_lat.as_deref(),
            px,
        )
    }

    fn shader_coord_mode(&self) -> u32 {
        match self.ordering {
            HealpixOrder::Ring => 4,
            HealpixOrder::Nested => 5,
        }
    }

    fn is_global(&self) -> bool {
        true
    }

    fn is_global_extent(&self) -> bool {
        true
    }

    fn lon_bounds_deg(&self) -> (f32, f32) {
        (0.0, 360.0)
    }

    fn lat_bounds_deg(&self) -> (f32, f32) {
        (-90.0, 90.0)
    }
}
