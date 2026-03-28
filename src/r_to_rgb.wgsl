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
    reference_white_scale: f32,
    output_scale: f32,
    tone_map_mode: u32, // 0: passthrough, 1: ACES, 2: GT7
};

@group(0) @binding(4)
var<uniform> params: Params;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
    let r = sample_channel(r_tex, uv);
    let g = sample_channel(g_tex, uv);
    let b = sample_channel(b_tex, uv);
    let color = vec3<f32>(r, g, b) * params.exposure;
    if params.tone_map_mode == 1u {
        let mapped = aces_hdr(color, params.output_scale) * params.reference_white_scale;
        return vec4<f32>(mapped, 1.0);
    }
    if params.tone_map_mode == 2u {
        let mapped = gt7_tonemap(color, params.output_scale) * params.reference_white_scale;
        return vec4<f32>(mapped, 1.0);
    }
    return vec4<f32>(color * params.reference_white_scale, 1.0);
}
