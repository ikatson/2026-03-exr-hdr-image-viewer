@group(0) @binding(0)
var r_tex: texture_2d<f32>;

@group(0) @binding(1)
var g_tex: texture_2d<f32>;

@group(0) @binding(2)
var b_tex: texture_2d<f32>;

@group(0) @binding(3)
var pick_sampler: sampler;

struct PickParams {
    uv: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(4)
var<uniform> pick_params: PickParams;

@group(0) @binding(5)
var<storage, read_write> out_color: array<vec4<f32>, 1>;

fn sample_channel(tex: texture_2d<f32>, uv: vec2<f32>) -> f32 {
    return textureSampleLevel(tex, pick_sampler, uv, 0.0).r;
}

@compute @workgroup_size(1, 1, 1)
fn cs_main() {
    let uv = pick_params.uv;
    let r = sample_channel(r_tex, uv);
    let g = sample_channel(g_tex, uv);
    let b = sample_channel(b_tex, uv);
    out_color[0] = vec4<f32>(r, g, b, 1.0);
}
