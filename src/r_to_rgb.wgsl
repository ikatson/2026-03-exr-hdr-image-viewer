@group(0) @binding(0)
var r_tex: texture_2d<f32>;

@group(0) @binding(1)
var g_tex: texture_2d<f32>;

@group(0) @binding(2)
var b_tex: texture_2d<f32>;

@group(0) @binding(3)
var channel_sampler: sampler;

struct Params {
    exposure: f32,
    sdr_white_nits: f32,
    peak_luma_nits: f32,
    nits_to_output_scale: f32,
    tone_map_mode: u32,
    is_yuv: u32,
};

@group(0) @binding(4)
var<uniform> params: Params;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct FsOut {
    @location(0) color: vec4<f32>,
    @location(1) debug: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );

    let pos = positions[vertex_index];

    var out: VsOut;
    out.position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = pos * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

fn sample_channel(tex: texture_2d<f32>, uv: vec2<f32>) -> f32 {
    return textureSampleLevel(tex, channel_sampler, uv, 0.0).r;
}

fn mul_rows3(
    r0: vec3<f32>,
    r1: vec3<f32>,
    r2: vec3<f32>,
    v: vec3<f32>,
) -> vec3<f32> {
    return vec3<f32>(dot(r0, v), dot(r1, v), dot(r2, v));
}

fn log10f(x: f32) -> f32 {
    return log(x) / log(10.0);
}

fn rrt_odt_curve(x: f32) -> f32 {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return (x * (a * x + b)) / (x * (c * x + d) + e);
}

fn rrt_odt_curve_v(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        rrt_odt_curve(c.x),
        rrt_odt_curve(c.y),
        rrt_odt_curve(c.z),
    );
}

fn aces_peak_curve(x: f32, peak: f32) -> f32 {
    let safe_peak = max(peak, 1e-5);
    let normalized = rrt_odt_curve(x / safe_peak);
    let peak_norm = 2.43 / 2.51;
    return normalized * safe_peak * peak_norm;
}

fn aces_hdr(color: vec3<f32>, peak: f32) -> vec3<f32> {
    let c = max(color, vec3<f32>(0.0));
    return vec3<f32>(
        aces_peak_curve(c.x, peak),
        aces_peak_curve(c.y, peak),
        aces_peak_curve(c.z, peak),
    );
}

fn gt7_tonemap_component(x: f32, peak: f32) -> f32 {
    let p = peak;
    let a = 1.0;
    let m = 0.22;
    let l = 0.4;
    let c = 1.33;
    let b = 0.0;

    let l0 = ((p - m) * l) / a;
    let l1 = m + (1.0 - m) / a;
    let s0 = m + l0;
    let s1 = m + a * l0;
    let c2 = (a * p) / max(p - s1, 1e-5);
    let cp = -c2 / p;

    let w0 = 1.0 - smoothstep(0.0, m, x);
    let w2 = select(0.0, 1.0, x >= m + l0);
    let w1 = 1.0 - w0 - w2;

    let toe = m * pow(max(x, 0.0) / max(m, 1e-5), c) + b;
    let linear = m + a * (x - m);
    let shoulder = p - (p - s1) * exp(cp * (x - s0));
    return toe * w0 + linear * w1 + shoulder * w2;
}

fn gt7_tonemap(x: vec3<f32>, peak: f32) -> vec3<f32> {
    return vec3<f32>(
        gt7_tonemap_component(x.r, peak),
        gt7_tonemap_component(x.g, peak),
        gt7_tonemap_component(x.b, peak),
    );
}

fn reinhard_tonemap_component(x: f32) -> f32 {
    return x / (1.0 + x);
}

fn reinhard_tonemap(x: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        reinhard_tonemap_component(x.r),
        reinhard_tonemap_component(x.g),
        reinhard_tonemap_component(x.b),
    );
}

fn neutwo_tonemap(x: vec3<f32>, peak: f32) -> vec3<f32> {
    let p = peak;
    return (p * x) / sqrt(x * x + p * p);
}

fn tonemap_nits(color: vec3<f32>, tone_map_mode: u32, sdr_white_nits: f32, peak: f32) -> vec3<f32> {
    if tone_map_mode == 0u {
        return color;
    }
    if tone_map_mode == 1u {
        return neutwo_tonemap(color / sdr_white_nits, peak / sdr_white_nits) * sdr_white_nits;
    }
    if tone_map_mode == 2u {
        return gt7_tonemap(color / sdr_white_nits, peak / sdr_white_nits) * sdr_white_nits;
    }
    if tone_map_mode == 3u {
        return reinhard_tonemap(color / sdr_white_nits) * sdr_white_nits;
    }
    if tone_map_mode == 4u {
        return aces_hdr(color / sdr_white_nits, peak / sdr_white_nits) * sdr_white_nits;
    }
    if tone_map_mode == 7u {
        var c = pq_eotf_inv(color);
        c = apply_bt2390_eetf(c, 0.01, params.peak_luma_nits);
        c = pq_eotf(c);
        return c;
    }
    return color;
}

fn convert_yuv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    // this is based on bt2020 doc
    // https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.2020-2-201510-I!!PDF-E.pdf

    // dequantize.
    let y = (c.x / 4. - 16.) / 219.;
    let cb = (c.y / 4. - 128.) / 224.;
    let cr = (c.z / 4. - 128.) / 224.;

    // extract r, g, b
    let r = cr * 1.4746 + y;
    let b = cb * 1.8814 + y;
    let g = (y - 0.2627 * r - 0.0593 * b) / 0.6780;

    return vec3<f32>(r, g, b);
}

// BT 2100 document
const PQ_m1: f32 = 0.1593017578125;
const PQ_m2: f32 = 78.84375;
const PQ_c1: f32 = 0.8359375;
const PQ_c2: f32 = 18.8515625;
const PQ_c3: f32 = 18.6875;

// PQ -> nits
fn pq_eotf_component(c: f32) -> f32 {
    let e_tmp = pow(c, 1. / PQ_m2);
    return pow(
        max(e_tmp - PQ_c1, 0.) / (PQ_c2 - PQ_c3 * e_tmp),
        1. / PQ_m1
    ) * 10000.;
}

// nits -> PQ
fn pq_eotf_inv_component(nits: f32) -> f32 {
    let y = nits / 10000.;
    let y_m = pow(y, PQ_m1);
    return pow((PQ_c1 + PQ_c2 * y_m) / (1. + PQ_c3 * y_m), PQ_m2);
}

fn pq_eotf_inv(c: vec3<f32>) -> vec3<f32> {
    return vec3(
        pq_eotf_inv_component(c.r),
        pq_eotf_inv_component(c.g),
        pq_eotf_inv_component(c.b),
    );
}

fn pq_eotf(c: vec3<f32>) -> vec3<f32> {
    // [0-1] non-lilnear (PQ) rgb -> [0-10000] absolute nits
    return vec3(
        pq_eotf_component(c.r),
        pq_eotf_component(c.g),
        pq_eotf_component(c.b),
    );
}

fn bt2020_to_709(c: vec3<f32>) -> vec3<f32> {
    // https://www.itu.int/dms_pub/itu-r/opb/rep/R-REP-BT.2407-2017-PDF-E.pdf
    //
    // column major
    return mat3x3<f32>(
        vec3(1.6605, -0.1246, -0.0182),
        vec3(-0.5876, 1.1329, -0.1006),
        vec3(-0.0728, -0.0083, 1.1187)
    ) * c;
}

// this tonemaps PQ into max display range
// input MUST be PQ itself (0-1), nonlinear
fn apply_bt2390_eetf(e_prime: vec3<f32>, display_min_nits: f32, display_max_nits: f32) -> vec3<f32> {
    // --- Step 0: Setup Parameters ---
    // Assuming content was mastered for the full PQ range if LB/LW are unknown.
    // lw might be maxfall .e.g 4000 or 1000
    let lb = 0.0;
    let lw = 4000.0;

    // Target display (Your monitor)
    let l_min = display_min_nits;   // Typical LCD black floor in nits
    let l_max = display_max_nits; // Your empirical 300 nits peak

    // Helper: Normalized PQ inverse (eotf^-1)
    let e_1 = (e_prime - pq_eotf_inv_component(lb)) / (pq_eotf_inv_component(lw) - pq_eotf_inv_component(lb));

    // --- Step 1: Calculate minLum and maxLum ---
    // These represent the target display's range relative to the mastering display
    // Using the 0-1 PQ space directly
    let min_lum = pq_eotf_inv_component(l_min); // eotf^-1(l_min) / 10000 normalized
    let max_lum = pq_eotf_inv_component(l_max);

    // --- Step 2: Calculate Knee Start (KS) and Lift (b) ---
    let ks = 1.5 * max_lum - 0.5;
    let b = min_lum;

    // --- Step 3 & 4: Solve for E3 (The Spline) ---
    return vec3<f32>(
        eetf_component(e_1.r, ks, max_lum, b),
        eetf_component(e_1.g, ks, max_lum, b),
        eetf_component(e_1.b, ks, max_lum, b)
    );
}

fn eetf_component(e1: f32, ks: f32, max_lum: f32, b: f32) -> f32 {
    var e2: f32;
    if (e1 < ks) {
        e2 = e1;
    } else {
        // Hermite Spline P[B]
        let t = (e1 - ks) / (1.0 - ks);
        e2 = (2.0*t*t*t - 3.0*t*t + 1.0) * ks +
             (t*t*t - 2.0*t*t + t) * (1.0 - ks) +
             (-2.0*t*t*t + 3.0*t*t) * max_lum;
    }

    // Final Black Level Lift (Step 3 final equation)
    // e3 = e2 + b(1 - e2)^4
    return e2 + b * pow(1.0 - e2, 4.0);
}

@fragment
fn fs_main(in: VsOut) -> FsOut {
    let uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
    let r = sample_channel(r_tex, uv);
    let g = sample_channel(g_tex, uv);
    let b = sample_channel(b_tex, uv);
    var color = vec3<f32>(r, g, b);
    var debug_value = color;

    if params.is_yuv == 1 {
        // [0-65535] -> [0-1] rgb
        color = convert_yuv_to_rgb(color);
        // [0-1] rgb -> [0-10000] linear
        color = pq_eotf(color);
        color = bt2020_to_709(color);
    } else {
        color = color * params.sdr_white_nits;
    }
    color *= params.exposure;
    color = tonemap_nits(color, params.tone_map_mode, params.sdr_white_nits, params.peak_luma_nits);
    debug_value = color;
    color = color * params.nits_to_output_scale;
    return FsOut(vec4<f32>(color, 1.0), vec4<f32>(debug_value, 1.0));
}
