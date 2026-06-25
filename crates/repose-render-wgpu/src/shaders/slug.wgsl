const HEADER_WORDS: u32 = 12u;

struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

@group(1) @binding(0) var<storage> glyph_data: array<u32>;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) @interpolate(flat) glyph_base: u32,
    @location(2) origin_px: vec2<f32>,
    @location(3) pivot_px: vec2<f32>,
    @location(4) cos_sin: vec2<f32>,
};

fn f32_from(i: u32) -> f32 {
    return bitcast<f32>(glyph_data[i]);
}

@vertex
fn vs_main(
    @location(0) center: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) glyph_base: u32,
    @location(4) origin_px: vec2<f32>,
    @location(5) pivot_px: vec2<f32>,
    @location(6) cos_sin: vec2<f32>,
    @builtin(vertex_index) vi: u32,
) -> VSOut {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0)
    );
    let c = corners[vi];
    let ndc_pos = center + c * 0.5 * size;

    return VSOut(
        vec4(ndc_pos, 0.0, 1.0),
        color,
        glyph_base,
        origin_px,
        pivot_px,
        cos_sin,
    );
}

fn solve_quadratic(a: f32, b: f32, c: f32) -> vec2<f32> {
    if abs(a) < 1e-10 {
        if abs(b) < 1e-10 { return vec2(1e10, 1e10); }
        let t = -c / b;
        return vec2(t, t);
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 { return vec2(1e10, 1e10); }
    let sd = sqrt(disc);
    if abs(b) < 1e-10 {
        // Degenerate Citardauq: sign(0) = 0 -> q = 0 -> division by zero.
        return vec2(-sd / (2.0 * a), sd / (2.0 * a));
    }
    let q = -0.5 * (b + sign(b) * sd);
    return vec2(c / q, q / a);
}

fn eval_horiz(curve_base: u32, rc: vec2<f32>, font_size: f32) -> f32 {
    let ax = f32_from(curve_base) - rc.x;
    let ay = f32_from(curve_base + 1u) - rc.y;
    let bx = f32_from(curve_base + 2u) - rc.x;
    let by = f32_from(curve_base + 3u) - rc.y;
    let cx = f32_from(curve_base + 4u) - rc.x;
    let cy = f32_from(curve_base + 5u) - rc.y;

    let c0 = ay;
    let c1 = 2.0 * (by - ay);
    let c2 = cy - 2.0 * by + ay;

    if abs(c2) < 1e-10 {
        if abs(c1) < 1e-10 { return 0.0; }
        let t = -c0 / c1;
        if t < 0.0 || t > 1.0 { return 0.0; }
        let xt = (1.0 - t) * (1.0 - t) * ax + 2.0 * (1.0 - t) * t * bx + t * t * cx;
        let cov = clamp(0.5 - xt * font_size, 0.0, 1.0);
        return sign(c1) * cov;
    }

    let roots = solve_quadratic(c2, c1, c0);
    var cov = 0.0;
    for (var i = 0u; i < 2u; i++) {
        let t = roots[i];
        if t >= 0.0 && t <= 1.0 {
            let xt = (1.0 - t) * (1.0 - t) * ax + 2.0 * (1.0 - t) * t * bx + t * t * cx;
            let dy_dt = 2.0 * c2 * t + c1;
            let sign = select(sign(dy_dt), 1.0, abs(dy_dt) < 1e-10);
            cov += sign * clamp(0.5 - xt * font_size, 0.0, 1.0);
        }
    }
    return cov;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let base = in.glyph_base;
    let header = glyph_data[base];
    let h_count = header & 0xFFu;
    let v_count = (header >> 8u) & 0xFFu;
    let num_bands = h_count + v_count;
    let font_size = f32_from(base + 10u);
    let sx = f32_from(base + 1u);
    let sy = f32_from(base + 2u);
    let ox = f32_from(base + 3u);
    let oy = f32_from(base + 4u);
    let band_scale = vec2(sx, sy);
    let band_offset = vec2(ox, oy);

    let h_max = h_count;

    let px_pos = in.pos.xy;
    let d = px_pos - in.pivot_px;
    let cos_a = in.cos_sin.x;
    let sin_a = in.cos_sin.y;
    let unrot = in.pivot_px + vec2(
        d.x * cos_a + d.y * sin_a,
        -d.x * sin_a + d.y * cos_a,
    );
    let rc = vec2(
        (unrot.x - in.origin_px.x) / font_size,
        (in.origin_px.y - unrot.y) / font_size,
    );

    let band_h = u32(clamp(rc.y * band_scale.y + band_offset.y, 0.0, f32(h_max - 1u)));

    let curve_start = glyph_data[base + 11u];

    var h_cov = 0.0;
    if (band_h < h_max) {
        let hdr = glyph_data[base + HEADER_WORDS + band_h];
        let count = hdr & 0xFFFFu;
        let ref_off = (hdr >> 16u);
        let ref_base = base + HEADER_WORDS + num_bands;
        for (var ri = 0u; ri < count; ri++) {
            let ci = glyph_data[ref_base + ref_off + ri];
            h_cov += eval_horiz(curve_start + ci * 6u, rc, font_size);
        }
    }

    let coverage = clamp(abs(h_cov), 0.0, 1.0);
    let a = in.color.a * coverage;
    return vec4(in.color.rgb * a, a);
}
