//! Bitwise Morton curve and Nested <-> Ring conversion schemes.

use super::nside_to_npix;

const JRLL: [isize; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
const JPLL: [isize; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];

/// Parallel bitmask 2D Morton interleave (Z-order curve) in O(1) time.
#[inline]
pub fn morton_interleave2(x: usize, y: usize) -> usize {
    let mut x = (x as u32) & 0x0000FFFF;
    let mut y = (y as u32) & 0x0000FFFF;

    x = (x | (x << 8)) & 0x00FF00FF;
    x = (x | (x << 4)) & 0x0F0F0F0F;
    x = (x | (x << 2)) & 0x33333333;
    x = (x | (x << 1)) & 0x55555555;

    y = (y | (y << 8)) & 0x00FF00FF;
    y = (y | (y << 4)) & 0x0F0F0F0F;
    y = (y | (y << 2)) & 0x33333333;
    y = (y | (y << 1)) & 0x55555555;

    (x | (y << 1)) as usize
}

/// Parallel bitmask 2D Morton deinterleave (Z-order curve) in O(1) time.
#[inline]
pub fn morton_deinterleave2(code: usize) -> (usize, usize) {
    let code = code as u32;
    let mut x = code & 0x55555555;
    let mut y = (code >> 1) & 0x55555555;

    x = (x | (x >> 1)) & 0x33333333;
    x = (x | (x >> 2)) & 0x0F0F0F0F;
    x = (x | (x >> 4)) & 0x00FF00FF;
    x = (x | (x >> 8)) & 0x0000FFFF;

    y = (y | (y >> 1)) & 0x33333333;
    y = (y | (y >> 2)) & 0x0F0F0F0F;
    y = (y | (y >> 4)) & 0x00FF00FF;
    y = (y | (y >> 8)) & 0x0000FFFF;

    (x as usize, y as usize)
}

/// Converts a pixel index from NESTED scheme to RING scheme.
pub fn nest2ring(nside: usize, pix_nest: usize) -> usize {
    let npix = nside_to_npix(nside);
    if nside <= 1 {
        return pix_nest.min(npix.saturating_sub(1));
    }
    let p_nest = pix_nest.min(npix.saturating_sub(1));
    let nside_sq = nside * nside;
    let face = (p_nest / nside_sq).min(11);
    let in_face = p_nest % nside_sq;

    let (ix, iy) = morton_deinterleave2(in_face);

    let nside_i = nside as isize;
    let nl4 = 4 * nside_i;
    let ncap = 2 * nside * (nside - 1);

    let jr = JRLL[face] * nside_i - ix as isize - iy as isize - 1;

    if jr < nside_i {
        let nr = jr;
        let mut ip = (JPLL[face] * nr + ix as isize - iy as isize + 1) / 2;
        if ip > 4 * nr {
            ip -= 4 * nr;
        }
        if ip < 1 {
            ip += 4 * nr;
        }
        let p_start = 2 * (nr as usize) * ((nr - 1) as usize);
        p_start + (ip as usize - 1)
    } else if jr <= 3 * nside_i {
        let kshift = (jr - nside_i) & 1;
        let mut ip = (JPLL[face] * nside_i + ix as isize - iy as isize + 1 + kshift) / 2;
        if ip > nl4 {
            ip -= nl4;
        }
        if ip < 1 {
            ip += nl4;
        }
        let p_start = ncap + ((jr - nside_i) as usize) * (4 * nside);
        p_start + (ip as usize - 1)
    } else {
        let nr = 4 * nside_i - jr;
        let mut ip = (JPLL[face] * nr + ix as isize - iy as isize + 1) / 2;
        if ip > 4 * nr {
            ip -= 4 * nr;
        }
        if ip < 1 {
            ip += 4 * nr;
        }
        let p_start = npix - 2 * (nr as usize) * ((nr + 1) as usize);
        p_start + (ip as usize - 1)
    }
}

/// Converts a pixel index from RING scheme to NESTED scheme.
pub fn ring2nest(nside: usize, pix_ring: usize) -> usize {
    let npix = nside_to_npix(nside);
    if nside <= 1 {
        return pix_ring.min(npix.saturating_sub(1));
    }
    let p_ring = pix_ring.min(npix.saturating_sub(1));
    let nside_i = nside as isize;
    let nl4 = 4 * nside_i;

    let (r, i) = super::math::pix2ring(nside, p_ring);
    let jr = r as isize;
    let ip = (i + 1) as isize;

    for face in 0..12 {
        let (ix_isize, iy_isize) = if jr < nside_i {
            if face >= 4 {
                continue;
            }
            let nr = jr;
            let jpll = JPLL[face];
            let diff = 2 * ip - jpll * nr - 1;
            let sum = 2 * nside_i - 1 - nr;
            ((sum + diff) / 2, (sum - diff) / 2)
        } else if jr <= 3 * nside_i {
            let kshift = (jr - nside_i) & 1;
            let jpll = JPLL[face];
            let jrll = JRLL[face];
            let mut found = None;
            for &wrap in &[0, nl4, -nl4] {
                let ip_adj = ip + wrap;
                let diff = 2 * ip_adj - jpll * nside_i - 1 - kshift;
                let sum = jrll * nside_i - 1 - jr;
                let ix = (sum + diff) / 2;
                let iy = (sum - diff) / 2;
                if ix >= 0 && ix < nside_i && iy >= 0 && iy < nside_i {
                    found = Some((ix, iy));
                    break;
                }
            }
            if let Some(coords) = found {
                coords
            } else {
                continue;
            }
        } else {
            if face < 8 {
                continue;
            }
            let nr = 4 * nside_i - jr;
            let jpll = JPLL[face];
            let diff = 2 * ip - jpll * nr - 1;
            let sum = nr - 1;
            ((sum + diff) / 2, (sum - diff) / 2)
        };

        if ix_isize >= 0 && ix_isize < nside_i && iy_isize >= 0 && iy_isize < nside_i {
            let in_face = morton_interleave2(ix_isize as usize, iy_isize as usize);
            return face * (nside * nside) + in_face;
        }
    }

    0
}
