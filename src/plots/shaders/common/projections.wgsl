// Shared geographic and spherical projection utilities for Octant WGSL shaders

fn lon_lat_to_cartesian(radius: f32, lon: f32, lat: f32) -> vec3<f32> {
    let cos_lat = cos(lat);
    let sin_lat = sin(lat);

    let x = radius * cos_lat * sin(lon);
    let y = radius * sin_lat;
    let z = radius * cos_lat * cos(lon);

    return vec3<f32>(x, y, z);
}

const JRLL = array<i32, 12>(2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4);
const JPLL = array<i32, 12>(1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7);

fn morton_compact_bits(v_in: u32) -> u32 {
    var v = v_in & 0x55555555u;
    v = (v ^ (v >> 1u)) & 0x33333333u;
    v = (v ^ (v >> 2u)) & 0x0F0F0F0Fu;
    v = (v ^ (v >> 4u)) & 0x00FF00FFu;
    v = (v ^ (v >> 8u)) & 0x0000FFFFu;
    return v;
}

fn morton_spread_bits(v_in: u32) -> u32 {
    var v = v_in & 0x0000FFFFu;
    v = (v ^ (v << 8u)) & 0x00FF00FFu;
    v = (v ^ (v << 4u)) & 0x0F0F0F0Fu;
    v = (v ^ (v << 2u)) & 0x33333333u;
    v = (v ^ (v << 1u)) & 0x55555555u;
    return v;
}

fn morton_deinterleave2(code: u32) -> vec2<u32> {
    return vec2<u32>(morton_compact_bits(code), morton_compact_bits(code >> 1u));
}

fn morton_interleave2(ix: u32, iy: u32) -> u32 {
    return morton_spread_bits(ix) | (morton_spread_bits(iy) << 1u);
}

fn healpix_nest2ring(nside: u32, pix_nest: u32) -> u32 {
    let npix = 12u * nside * nside;
    if (nside <= 1u) {
        return min(pix_nest, npix - 1u);
    }
    let p_nest = min(pix_nest, npix - 1u);
    let nside_sq = nside * nside;
    let face = min(p_nest / nside_sq, 11u);
    let in_face = p_nest % nside_sq;

    let coords = morton_deinterleave2(in_face);
    let ix = coords.x;
    let iy = coords.y;

    let nside_i = i32(nside);
    let nl4 = 4 * nside_i;
    let ncap = 2u * nside * (nside - 1u);

    let jr = JRLL[face] * nside_i - i32(ix) - i32(iy) - 1;

    if (jr < nside_i) {
        let nr = jr;
        var ip = (JPLL[face] * nr + i32(ix) - i32(iy) + 1) / 2;
        if (ip > 4 * nr) {
            ip = ip - 4 * nr;
        }
        if (ip < 1) {
            ip = ip + 4 * nr;
        }
        let p_start = 2u * u32(nr) * u32(nr - 1);
        return p_start + u32(ip - 1);
    } else if (jr <= 3 * nside_i) {
        let kshift = (jr - nside_i) & 1;
        var ip = (JPLL[face] * nside_i + i32(ix) - i32(iy) + 1 + kshift) / 2;
        if (ip > nl4) {
            ip = ip - nl4;
        }
        if (ip < 1) {
            ip = ip + nl4;
        }
        let p_start = ncap + u32(jr - nside_i) * (4u * nside);
        return p_start + u32(ip - 1);
    } else {
        let nr = 4 * nside_i - jr;
        var ip = (JPLL[face] * nr + i32(ix) - i32(iy) + 1) / 2;
        if (ip > 4 * nr) {
            ip = ip - 4 * nr;
        }
        if (ip < 1) {
            ip = ip + 4 * nr;
        }
        let p_start = npix - 2u * u32(nr) * u32(nr + 1);
        return p_start + u32(ip - 1);
    }
}

fn healpix_ring2nest(nside: u32, pix_ring: u32) -> u32 {
    let npix = 12u * nside * nside;
    if (nside <= 1u) {
        return min(pix_ring, npix - 1u);
    }
    let p_ring = min(pix_ring, npix - 1u);
    let nside_i = i32(nside);
    let nl4 = 4 * nside_i;
    let ncap = 2u * nside * (nside - 1u);

    var jr = 0;
    var ip = 0;
    if (p_ring < ncap) {
        let r = max(i32(floor((1.0 + sqrt(1.0 + 2.0 * f32(p_ring))) * 0.5)), 1);
        let p_start = 2u * u32(r) * u32(r - 1);
        jr = r;
        ip = i32(p_ring - p_start + 1u);
    } else if (p_ring < npix - ncap) {
        let p_eq = p_ring - ncap;
        let r_eq = i32(p_eq / (4u * nside));
        jr = nside_i + r_eq;
        ip = i32(p_eq % (4u * nside) + 1u);
    } else {
        let p_south = (npix - 1u) - p_ring;
        let r_south = max(i32(floor((1.0 + sqrt(1.0 + 2.0 * f32(p_south))) * 0.5)), 1);
        jr = 4 * nside_i - r_south;
        let p_start = npix - 2u * u32(r_south) * u32(r_south + 1);
        ip = i32(p_ring - p_start + 1u);
    }

    for (var face = 0u; face < 12u; face = face + 1u) {
        var ix_isize = -1;
        var iy_isize = -1;
        if (jr < nside_i) {
            if (face >= 4u) {
                continue;
            }
            let nr = jr;
            let jpll = JPLL[face];
            let diff = 2 * ip - jpll * nr - 1;
            let sum = 2 * nside_i - 1 - nr;
            ix_isize = (sum + diff) / 2;
            iy_isize = (sum - diff) / 2;
        } else if (jr <= 3 * nside_i) {
            let kshift = (jr - nside_i) & 1;
            let jpll = JPLL[face];
            let jrll = JRLL[face];
            let wraps = array<i32, 3>(0, nl4, -nl4);
            for (var w = 0u; w < 3u; w = w + 1u) {
                let ip_adj = ip + wraps[w];
                let diff = 2 * ip_adj - jpll * nside_i - 1 - kshift;
                let sum = jrll * nside_i - 1 - jr;
                let test_ix = (sum + diff) / 2;
                let test_iy = (sum - diff) / 2;
                if (test_ix >= 0 && test_ix < nside_i && test_iy >= 0 && test_iy < nside_i) {
                    ix_isize = test_ix;
                    iy_isize = test_iy;
                    break;
                }
            }
        } else {
            if (face < 8u) {
                continue;
            }
            let nr = 4 * nside_i - jr;
            let jpll = JPLL[face];
            let diff = 2 * ip - jpll * nr - 1;
            let sum = nr - 1;
            ix_isize = (sum + diff) / 2;
            iy_isize = (sum - diff) / 2;
        }

        if (ix_isize >= 0 && ix_isize < nside_i && iy_isize >= 0 && iy_isize < nside_i) {
            let in_face = morton_interleave2(u32(ix_isize), u32(iy_isize));
            return face * (nside * nside) + in_face;
        }
    }
    return 0u;
}

fn healpix_pix2ang_ring(nside: u32, pix: u32) -> vec2<f32> {
    let ns = max(nside, 1u);
    let ns_f = f32(ns);
    let npix = 12u * ns * ns;
    let p = min(pix, npix - 1u);
    let ncap = 2u * ns * (ns - 1u);
    let pi = 3.14159265;

    if (p < ncap) {
        // North Polar Cap
        let r = max(u32(floor((1.0 + sqrt(1.0 + 2.0 * f32(p))) * 0.5)), 1u);
        let r_f = f32(r);
        let p_start = 2u * r * (r - 1u);
        let i = p - p_start;
        let z = 1.0 - (r_f * r_f) / (3.0 * ns_f * ns_f);
        let lat = asin(clamp(z, -1.0, 1.0));
        let lon = ((f32(i) + 0.5) * pi / (2.0 * r_f));
        return vec2<f32>(lon, lat);
    } else if (p < npix - ncap) {
        // Equatorial Belt
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
        // South Polar Cap
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

/// Continuous transformation from base face diamond (x, y) coordinates to spherical (lon, lat)
fn healpix_face_xy_to_lon_lat(face: u32, x: f32, y: f32, nside: u32) -> vec2<f32> {
    let nside_f = f32(max(nside, 1u));
    let jr = f32(JRLL[face]) * nside_f - x - y;
    let pi = 3.14159265;
    let two_pi = 6.2831853;

    var z: f32;
    var lon: f32;

    if (jr < nside_f) {
        // North Polar Cap
        let nr = max(jr, 1e-6);
        let jp = (f32(JPLL[face]) * nr + x - y) * 0.5;
        z = 1.0 - (nr * nr) / (3.0 * nside_f * nside_f);
        lon = jp * (pi / (2.0 * nr));
    } else if (jr <= 3.0 * nside_f) {
        // Equatorial Belt
        let jp = (f32(JPLL[face]) * nside_f + x - y) * 0.5;
        z = (2.0 / 3.0) * (2.0 - jr / nside_f);
        lon = jp * (pi / (2.0 * nside_f));
    } else {
        // South Polar Cap
        let nr = max(4.0 * nside_f - jr, 1e-6);
        let jp = (f32(JPLL[face]) * nr + x - y) * 0.5;
        z = -(1.0 - (nr * nr) / (3.0 * nside_f * nside_f));
        lon = jp * (pi / (2.0 * nr));
    }

    let lat = asin(clamp(z, -1.0, 1.0));
    let lon_wrapped = (lon % two_pi + two_pi) % two_pi;
    return vec2<f32>(lon_wrapped, lat);
}

/// Evaluates continuous (lon, lat) at normalized diamond coordinates uv within a HEALPix cell
fn healpix_pixel_uv_to_lon_lat(pix: u32, uv: vec2<f32>, nside: u32, is_nested: bool) -> vec2<f32> {
    let ns = max(nside, 1u);
    var p_nest = pix;
    if (!is_nested) {
        p_nest = healpix_ring2nest(ns, pix);
    }
    let nside_sq = ns * ns;
    let face = min(p_nest / nside_sq, 11u);
    let in_face = p_nest % nside_sq;

    let coords = morton_deinterleave2(in_face);
    let x = f32(coords.x) + uv.x;
    let y = f32(coords.y) + uv.y;
    return healpix_face_xy_to_lon_lat(face, x, y, ns);
}

/// Evaluates smoothly interpolated corner values across HEALPix cell boundaries for continuous terrain
fn healpix_get_interpolated_corner_val(
    pix: u32,
    corner_uv: vec2<f32>,
    nside: u32,
    is_nested: bool,
    max_idx: u32,
) -> f32 {
    let ns = max(nside, 1u);
    var p_nest = pix;
    if (!is_nested) {
        p_nest = healpix_ring2nest(ns, pix);
    }
    let nside_sq = ns * ns;
    let face = min(p_nest / nside_sq, 11u);
    let in_face = p_nest % nside_sq;

    let coords = morton_deinterleave2(in_face);
    let cx = coords.x + u32(round(corner_uv.x));
    let cy = coords.y + u32(round(corner_uv.y));

    var sum: f32 = 0.0;
    var count: f32 = 0.0;

    let offsets_x = array<i32, 4>(-1, 0, -1, 0);
    let offsets_y = array<i32, 4>(-1, -1, 0, 0);

    for (var k = 0u; k < 4u; k = k + 1u) {
        let px_cand = i32(cx) + offsets_x[k];
        let py_cand = i32(cy) + offsets_y[k];

        if (px_cand >= 0 && px_cand < i32(ns) && py_cand >= 0 && py_cand < i32(ns)) {
            let cand_in_face = morton_interleave2(u32(px_cand), u32(py_cand));
            var cand_pix = face * nside_sq + cand_in_face;
            if (!is_nested) {
                cand_pix = healpix_nest2ring(ns, cand_pix);
            }
            let safe_cand = min(cand_pix, max_idx);
            let v = data_buffer[safe_cand];
            if (v == v && abs(v) < 1e30) {
                sum = sum + v;
                count = count + 1.0;
            }
        }
    }

    if (count > 0.0) {
        return sum / count;
    }
    return data_buffer[min(pix, max_idx)];
}

fn get_lon_lat(
    cell_x: u32,
    cell_y: u32,
    model_xy: vec2<f32>,
    grid_w: u32,
    grid_h: u32,
    coord_mode: u32,
    lon_bounds: vec2<f32>,
    lat_bounds: vec2<f32>,
) -> vec2<f32> {
    if (coord_mode == 0u) {
        // Mode 0: Global Regular [-π..π] and [π/2..-π/2]
        let u = (f32(cell_x) + model_xy.x) / f32(grid_w);
        let v = (f32(cell_y) + model_xy.y) / f32(grid_h);
        let lon = (u - 0.5) * 2.0 * 3.14159265;
        let lat = (0.5 - v) * 3.14159265;
        return vec2<f32>(lon, lat);
    } else if (coord_mode == 1u) {
        // Mode 1: Regional Regular with explicit [lon_bounds, lat_bounds]
        let u = select(
            (f32(cell_x) + model_xy.x - 0.5) / max(f32(grid_w) - 1.0, 1.0),
            model_xy.x,
            grid_w <= 1u
        );
        let v = select(
            (f32(cell_y) + model_xy.y - 0.5) / max(f32(grid_h) - 1.0, 1.0),
            model_xy.y,
            grid_h <= 1u
        );
        let lon = mix(lon_bounds.x, lon_bounds.y, u);
        let lat = clamp(mix(lat_bounds.y, lat_bounds.x, v), -1.5707963, 1.5707963);
        return vec2<f32>(lon, lat);
    } else if (coord_mode == 4u || coord_mode == 5u) {
        // Mode 4/5: HEALPix discrete global grid (4: Ring, 5: Nested)
        let pix = cell_y * grid_w + cell_x;
        let npix = max(grid_w * grid_h, 12u);
        let nside = max(u32(round(sqrt(f32(npix) / 12.0))), 1u);
        let is_nested = (coord_mode == 5u);
        let healpix_uv = vec2<f32>(model_xy.x, 1.0 - model_xy.y);
        return healpix_pixel_uv_to_lon_lat(pix, healpix_uv, nside, is_nested);
    } else {
        // Mode 2: Irregular 1D Coordinate Buffers with heatmap-matching interval boundaries
        let bounds_u = get_cell_normalized_bounds_x(cell_x, grid_w, coord_mode);
        let bounds_v = get_cell_normalized_bounds_y(cell_y, grid_h, coord_mode);
        let u = mix(bounds_u.x, bounds_u.y, model_xy.x);
        let v = mix(bounds_v.x, bounds_v.y, model_xy.y);

        let max_cx = min(grid_w - 1u, max(arrayLength(&coord_x_buffer), 1u) - 1u);
        let first_x = coord_x_buffer[0];
        let last_x = coord_x_buffer[max_cx];
        let deg_lon = mix(first_x, last_x, u);

        let max_cy = min(grid_h - 1u, max(arrayLength(&coord_y_buffer), 1u) - 1u);
        let first_y = coord_y_buffer[0];
        let last_y = coord_y_buffer[max_cy];
        let deg_lat = clamp(mix(first_y, last_y, v), -90.0, 90.0);

        return vec2<f32>(deg_lon * 0.0174532925, deg_lat * 0.0174532925);
    }
}
