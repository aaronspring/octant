// HEALPix spherical diamond coordinate projection and ang2pix/pix2ang mapping routines

fn healpix_pix2ang_ring(nside: u32, pix: u32) -> vec2<f32> {
    let ns = max(nside, 1u);
    let ns_f = f32(ns);
    let npix = 12u * ns * ns;
    let p = min(pix, npix - 1u);
    let ncap = 2u * ns * (ns - 1u);
    let pi = 3.14159265;

    if (p < ncap) {
        let r = max(u32(floor((1.0 + sqrt(1.0 + 2.0 * f32(p))) * 0.5)), 1u);
        let r_f = f32(r);
        let p_start = 2u * r * (r - 1u);
        let i = p - p_start;
        let z = 1.0 - (r_f * r_f) / (3.0 * ns_f * ns_f);
        let lat = asin(clamp(z, -1.0, 1.0));
        let lon = ((f32(i) + 0.5) * pi / (2.0 * r_f));
        return vec2<f32>(lon, lat);
    } else if (p < npix - ncap) {
        let p_eq = p - ncap;
        let r_eq = p_eq / (4u * ns);
        let r = ns + r_eq;
        let r_f = f32(r);
        let i = p_eq % (4u * ns);
        let z = 2.0 * (2.0 * ns_f - r_f) / (3.0 * ns_f);
        let lat = asin(clamp(z, -1.0, 1.0));
        let shift = select(0.0, 0.5, (r - ns) % 2u == 0u);
        let lon = ((f32(i) + shift) * pi / (2.0 * ns_f));
        return vec2<f32>(lon, lat);
    } else {
        let p_south = (npix - 1u) - p;
        let r_south = max(u32(floor((1.0 + sqrt(1.0 + 2.0 * f32(p_south))) * 0.5)), 1u);
        let r_f = f32(r_south);
        let p_start = npix - 2u * r_south * (r_south + 1u);
        let i = p - p_start;
        let z = -(1.0 - (r_f * r_f) / (3.0 * ns_f * ns_f));
        let lat = asin(clamp(z, -1.0, 1.0));
        let lon = ((f32(i) + 0.5) * pi / (2.0 * r_f));
        return vec2<f32>(lon, lat);
    }
}

fn healpix_ang2pix_ring(nside: u32, lon_rad: f32, lat_rad: f32) -> u32 {
    let ns = max(nside, 1u);
    let ns_f = f32(ns);
    let npix = 12u * ns * ns;
    let pi = 3.14159265;
    let two_pi = 6.2831853;
    let lon = (lon_rad % two_pi + two_pi) % two_pi;
    let z = clamp(sin(lat_rad), -1.0, 1.0);
    let za = abs(z);
    let tt = lon / (0.5 * pi);

    if (za <= 0.6666667) {
        let temp1 = ns_f * (0.5 + tt);
        let temp2 = ns_f * (0.75 * z);
        let jp = i32(floor(temp1 - temp2));
        let jm = i32(floor(temp1 + temp2));
        let ir = i32(ns) + 1 + jp - jm;
        let kshift = 1 - (ir & 1);
        let ip_raw = (jp + jm - i32(ns) + kshift + 1) / 2;
        let num_pixels = i32(4u * ns);
        let ip = u32((ip_raw % num_pixels + num_pixels) % num_pixels);
        let ncap = 2u * ns * (ns - 1u);
        let ring_idx = u32(max(ir - 1, 0));
        return ncap + ring_idx * (4u * ns) + min(ip, 4u * ns - 1u);
    } else {
        let tp = tt - floor(tt);
        let tmp = ns_f * sqrt(max(3.0 * (1.0 - za), 0.0));
        let jp = u32(floor(tp * tmp));
        let jm = u32(floor((1.0 - tp) * tmp));
        let ir = jp + jm + 1u;
        var ip = u32(floor(tt * f32(ir)));
        if (ip >= 4u * ir) {
            ip -= 4u * ir;
        }
        if (z > 0.0) {
            return 2u * ir * (ir - 1u) + ip;
        } else {
            return npix - 2u * ir * (ir + 1u) + ip;
        }
    }
}

struct HealpixFaceCoords {
    face: u32,
    x: u32,
    y: u32,
}

fn healpix_pixel_to_face_coords(pix: u32, nside: u32, is_nested: bool) -> HealpixFaceCoords {
    let ns = max(nside, 1u);
    var p_nest = pix;
    if (!is_nested) {
        p_nest = healpix_ring2nest(ns, pix);
    }
    let nside_sq = ns * ns;
    let face = min(p_nest / nside_sq, 11u);
    let in_face = p_nest % nside_sq;
    let coords = morton_deinterleave2(in_face);
    return HealpixFaceCoords(face, coords.x, coords.y);
}

fn healpix_face_xy_to_lon_lat(face: u32, x: f32, y: f32, nside: u32) -> vec2<f32> {
    let nside_f = f32(max(nside, 1u));
    let inv_nside = 1.0 / nside_f;
    let inv_3_nside_sq = 1.0 / (3.0 * nside_f * nside_f);
    let jr = f32(JRLL[face]) * nside_f - x - y;
    let pi = 3.14159265;
    let two_pi = 6.2831853;

    var z: f32;
    var lon: f32;

    if (jr < nside_f) {
        let nr = max(jr, 1e-6);
        let jp = (f32(JPLL[face]) * nr + x - y) * 0.5;
        z = 1.0 - (nr * nr) * inv_3_nside_sq;
        lon = jp * (pi / (2.0 * nr));
    } else if (jr <= 3.0 * nside_f) {
        let jp = (f32(JPLL[face]) * nside_f + x - y) * 0.5;
        z = (2.0 / 3.0) * (2.0 - jr * inv_nside);
        lon = jp * (pi * 0.5 * inv_nside);
    } else {
        let nr = max(4.0 * nside_f - jr, 1e-6);
        let jp = (f32(JPLL[face]) * nr + x - y) * 0.5;
        z = -(1.0 - (nr * nr) * inv_3_nside_sq);
        lon = jp * (pi / (2.0 * nr));
    }

    let lat = asin(clamp(z, -1.0, 1.0));
    let lon_wrapped = (lon % two_pi + two_pi) % two_pi;
    return vec2<f32>(lon_wrapped, lat);
}

fn healpix_pixel_uv_to_lon_lat(pix: u32, uv: vec2<f32>, nside: u32, is_nested: bool) -> vec2<f32> {
    let fc = healpix_pixel_to_face_coords(pix, nside, is_nested);
    let x = f32(fc.x) + uv.x;
    let y = f32(fc.y) + uv.y;
    return healpix_face_xy_to_lon_lat(fc.face, x, y, nside);
}

fn healpix_nside(npix: u32) -> u32 {
    return max(u32(round(sqrt(f32(max(npix, 12u)) / 12.0))), 1u);
}

fn healpix_ang2pix(nside: u32, lon_rad: f32, lat_rad: f32, is_nested: bool) -> u32 {
    var pix = healpix_ang2pix_ring(nside, lon_rad, lat_rad);
    if (is_nested) {
        pix = healpix_ring2nest(nside, pix);
    }
    return pix;
}
