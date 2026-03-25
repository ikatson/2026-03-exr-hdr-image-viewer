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
    tone_map_mode: u32, // 0: passthrough, 1: reinhard
    _pad0: u32,
    _pad1: u32,
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
    let r = sample_channel(r_tex, uv);
    let g = sample_channel(g_tex, uv);
    let b = sample_channel(b_tex, uv);
    let color = vec3<f32>(r, g, b) * params.exposure;
    if params.tone_map_mode == 1u {
        let mapped = color / (vec3<f32>(1.0) + color);
        return vec4<f32>(mapped, 1.0);
    }
    return vec4<f32>(color, 1.0);
}
