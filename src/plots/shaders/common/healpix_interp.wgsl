// Interpolation routine across HEALPix diamond boundaries

fn healpix_get_interpolated_corner_val(
    pix: u32,
    corner_uv: vec2<f32>,
    nside: u32,
    is_nested: bool,
    max_idx: u32,
) -> f32 {
    let ns = max(nside, 1u);
    let fc = healpix_pixel_to_face_coords(pix, ns, is_nested);
    let nside_sq = ns * ns;
    let cx = fc.x + u32(round(corner_uv.x));
    let cy = fc.y + u32(round(corner_uv.y));

    var sum: f32 = 0.0;
    var count: f32 = 0.0;

    let offsets_x = array<i32, 4>(-1, 0, -1, 0);
    let offsets_y = array<i32, 4>(-1, -1, 0, 0);

    for (var k = 0u; k < 4u; k = k + 1u) {
        let px_cand = i32(cx) + offsets_x[k];
        let py_cand = i32(cy) + offsets_y[k];

        if (px_cand >= 0 && px_cand < i32(ns) && py_cand >= 0 && py_cand < i32(ns)) {
            let cand_in_face = morton_interleave2(u32(px_cand), u32(py_cand));
            var cand_pix = fc.face * nside_sq + cand_in_face;
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
