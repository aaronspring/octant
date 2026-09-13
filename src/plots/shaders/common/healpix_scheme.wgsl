// HEALPix bit manipulation and scheme conversions (Ring <-> Nested)

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
