//! HEALPix spherical projection math, ring pixel indexing, and cell boundary polygons.

use super::nside_to_npix;
use super::scheme::{nest2ring, ring2nest};
use std::f32::consts::{FRAC_PI_2, PI};

/// Converts a pixel index in RING scheme to spherical coordinates `(lon_rad, lat_rad)`.
pub fn pix2ang_ring(nside: usize, pix: usize) -> (f32, f32) {
    let nside = nside.max(1);
    let npix = nside_to_npix(nside);
    let p = pix.min(npix.saturating_sub(1));
    let ncap = 2 * nside * (nside.saturating_sub(1));

    if p < ncap {
        let r = (((1.0 + (1.0 + 2.0 * p as f64).sqrt()) * 0.5).floor() as usize).max(1);
        let p_ring_start = 2 * r * (r - 1);
        let i = p.saturating_sub(p_ring_start);

        let z = 1.0 - (r as f32 * r as f32) / (3.0 * nside as f32 * nside as f32);
        let lat = z.clamp(-1.0, 1.0).asin();
        let lon = ((i as f32 + 0.5) * PI / (2.0 * r as f32)).rem_euclid(2.0 * PI);
        (lon, lat)
    } else if p < npix.saturating_sub(ncap) {
        let p_eq = p - ncap;
        let r_eq = p_eq / (4 * nside);
        let r = nside + r_eq;
        let i = p_eq % (4 * nside);

        let z = 2.0 * (2.0 * nside as f32 - r as f32) / (3.0 * nside as f32);
        let lat = z.clamp(-1.0, 1.0).asin();

        let shift = if (r - nside).is_multiple_of(2) {
            0.5
        } else {
            0.0
        };
        let lon = ((i as f32 + shift) * PI / (2.0 * nside as f32)).rem_euclid(2.0 * PI);
        (lon, lat)
    } else {
        let p_south = (npix - 1).saturating_sub(p);
        let r_south = (((1.0 + (1.0 + 2.0 * p_south as f64).sqrt()) * 0.5).floor() as usize).max(1);
        let p_ring_start = npix - 2 * r_south * (r_south + 1);
        let i = p.saturating_sub(p_ring_start);

        let z = -(1.0 - (r_south as f32 * r_south as f32) / (3.0 * nside as f32 * nside as f32));
        let lat = z.clamp(-1.0, 1.0).asin();
        let lon = ((i as f32 + 0.5) * PI / (2.0 * r_south as f32)).rem_euclid(2.0 * PI);
        (lon, lat)
    }
}

/// Converts spherical coordinates `(lon_rad, lat_rad)` to the corresponding pixel index in RING scheme.
pub fn ang2pix_ring(nside: usize, lon_rad: f32, lat_rad: f32) -> usize {
    let nside = nside.max(1);
    let nside_f = nside as f64;
    let npix = nside_to_npix(nside);

    let z = lat_rad.sin().clamp(-1.0, 1.0) as f64;
    let za = z.abs();
    let lon = (lon_rad.rem_euclid(2.0 * PI)) as f64;
    let tt = lon / (0.5 * std::f64::consts::PI);

    if za <= 2.0 / 3.0 {
        let temp1 = nside_f * (0.5 + tt);
        let temp2 = nside_f * (0.75 * z);
        let jp = (temp1 - temp2).floor() as i64;
        let jm = (temp1 + temp2).floor() as i64;
        let ir = nside as i64 + 1 + jp - jm;
        let kshift = 1 - (ir & 1);
        let mut ip =
            ((jp + jm - nside as i64 + kshift + 1) / 2).rem_euclid(4 * nside as i64) as usize;
        if ip >= 4 * nside {
            ip = 4 * nside - 1;
        }

        let ncap = 2 * nside * (nside.saturating_sub(1));
        let ring_idx = (ir - 1).max(0) as usize;
        ncap + ring_idx * (4 * nside) + ip
    } else {
        let tp = tt - tt.floor();
        let tmp = nside_f * (3.0 * (1.0 - za)).max(0.0).sqrt();
        let jp = (tp * tmp).floor() as usize;
        let jm = ((1.0 - tp) * tmp).floor() as usize;
        let ir = jp + jm + 1;
        let mut ip = (tt * ir as f64).floor() as usize;
        if ip >= 4 * ir {
            ip -= 4 * ir;
        }

        if z > 0.0 {
            2 * ir * (ir - 1) + ip
        } else {
            npix - 2 * ir * (ir + 1) + ip
        }
    }
}

/// Returns the 1-based latitude ring index `(1 ..= 4 * nside - 1)` and 0-based pixel index in ring.
pub fn pix2ring(nside: usize, pix: usize) -> (usize, usize) {
    let nside = nside.max(1);
    let npix = nside_to_npix(nside);
    let p = pix.min(npix.saturating_sub(1));
    let ncap = 2 * nside * (nside.saturating_sub(1));

    if p < ncap {
        let r = (((1.0 + (1.0 + 2.0 * p as f64).sqrt()) * 0.5).floor() as usize).max(1);
        let p_ring_start = 2 * r * (r - 1);
        (r, p.saturating_sub(p_ring_start))
    } else if p < npix.saturating_sub(ncap) {
        let p_eq = p - ncap;
        let r_eq = p_eq / (4 * nside);
        let r = nside + r_eq;
        (r, p_eq % (4 * nside))
    } else {
        let p_south = (npix - 1).saturating_sub(p);
        let r_south = (((1.0 + (1.0 + 2.0 * p_south as f64).sqrt()) * 0.5).floor() as usize).max(1);
        let r = 4 * nside - r_south;
        let p_ring_start = npix - 2 * r_south * (r_south + 1);
        (r, p.saturating_sub(p_ring_start))
    }
}

/// Converts a pixel index from NESTED scheme to spherical coordinates `(lon_rad, lat_rad)`.
pub fn pix2ang_nest(nside: usize, pix_nest: usize) -> (f32, f32) {
    let p_ring = nest2ring(nside, pix_nest);
    pix2ang_ring(nside, p_ring)
}

/// Converts spherical coordinates `(lon_rad, lat_rad)` to the corresponding pixel index in NESTED scheme.
pub fn ang2pix_nest(nside: usize, lon_rad: f32, lat_rad: f32) -> usize {
    let p_ring = ang2pix_ring(nside, lon_rad, lat_rad);
    ring2nest(nside, p_ring)
}

/// Computes the 4 spherical corner vertices `[(lon, lat); 4]` of a HEALPix cell.
pub fn pix_boundaries(nside: usize, pix: usize) -> [(f32, f32); 4] {
    let (center_lon, center_lat) = pix2ang_ring(nside, pix);
    let d_lat = (PI / (4.0 * nside as f32)).min(FRAC_PI_2 * 0.5);
    let cos_lat = center_lat.cos().abs().max(0.1);
    let d_lon = (PI / (2.0 * nside as f32 * cos_lat)).min(PI);

    [
        (
            center_lon,
            (center_lat + d_lat).clamp(-FRAC_PI_2, FRAC_PI_2),
        ),
        ((center_lon + d_lon).rem_euclid(2.0 * PI), center_lat),
        (
            center_lon,
            (center_lat - d_lat).clamp(-FRAC_PI_2, FRAC_PI_2),
        ),
        ((center_lon - d_lon).rem_euclid(2.0 * PI), center_lat),
    ]
}
