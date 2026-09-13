//! Geometry equality check for CoordinateGrid.

use super::types::CoordinateGrid;
use std::sync::Arc;

/// Compares two CoordinateGrid instances for geometry equality.
pub fn is_same_geometry(left: &CoordinateGrid, right: &CoordinateGrid) -> bool {
    if left.render_coord_mode() != right.render_coord_mode()
        || left.lon_bounds_deg() != right.lon_bounds_deg()
        || left.lat_bounds_deg() != right.lat_bounds_deg()
    {
        return false;
    }

    match (left, right) {
        (
            CoordinateGrid::Irregular1D {
                coords_x: lx,
                coords_y: ly,
                ..
            },
            CoordinateGrid::Irregular1D {
                coords_x: rx,
                coords_y: ry,
                ..
            },
        ) => {
            (Arc::ptr_eq(lx, rx) || lx.as_ref() == rx.as_ref())
                && (Arc::ptr_eq(ly, ry) || ly.as_ref() == ry.as_ref())
        }
        (
            CoordinateGrid::Curvilinear2D {
                lons: lx, lats: ly, ..
            },
            CoordinateGrid::Curvilinear2D {
                lons: rx, lats: ry, ..
            },
        ) => {
            (Arc::ptr_eq(lx, rx) || lx.as_ref() == rx.as_ref())
                && (Arc::ptr_eq(ly, ry) || ly.as_ref() == ry.as_ref())
        }
        (
            CoordinateGrid::Healpix {
                nside: ln,
                ordering: lo,
                ..
            },
            CoordinateGrid::Healpix {
                nside: rn,
                ordering: ro,
                ..
            },
        ) => ln == rn && lo == ro,
        (CoordinateGrid::GlobalRegular, CoordinateGrid::GlobalRegular)
        | (CoordinateGrid::RegionalRegular { .. }, CoordinateGrid::RegionalRegular { .. }) => true,
        _ => false,
    }
}
