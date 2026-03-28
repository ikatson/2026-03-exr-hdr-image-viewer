const GLYPH_W: u32 = 5u;
const GLYPH_H: u32 = 7u;
const GLYPH_SCALE: u32 = 2u;
const GLYPH_ADVANCE: u32 = (GLYPH_W + 2u) * GLYPH_SCALE;
const MAX_TEXT_CHARS: u32 = 72u;
const ATLAS_COLUMNS: u32 = 16u;

struct OverlayParams {
    screen_size: vec2<f32>,
    text_size: vec2<f32>,
    offset: vec2<f32>,
    _pad: vec2<f32>,
};

struct TextData {
    header: vec4<u32>,
    glyph_indices: array<u32, MAX_TEXT_CHARS>,
};

@group(0) @binding(0)
var glyph_atlas: texture_2d<f32>;

@group(0) @binding(1)
var<uniform> params: OverlayParams;

@group(0) @binding(2)
var<storage, read> text_data: TextData;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var local = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let p = local[idx];
    let pixel = params.offset + p * params.text_size;
    let ndc = vec2<f32>(
        pixel.x / params.screen_size.x * 2.0 - 1.0,
        1.0 - pixel.y / params.screen_size.y * 2.0,
    );

    var out: VsOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = p;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let p = vec2<u32>(u32(in.uv.x * params.text_size.x), u32(in.uv.y * params.text_size.y));
    let char_idx = p.x / GLYPH_ADVANCE;
    let text_len = text_data.header.x;
    if char_idx >= text_len || char_idx >= MAX_TEXT_CHARS {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let x_in_char = p.x - char_idx * GLYPH_ADVANCE;
    let y_in_char = p.y;
    if x_in_char >= GLYPH_W * GLYPH_SCALE || y_in_char >= GLYPH_H * GLYPH_SCALE {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let gx = x_in_char / GLYPH_SCALE;
    let gy = y_in_char / GLYPH_SCALE;
    let glyph_idx = text_data.glyph_indices[char_idx];

    let atlas_x = (glyph_idx % ATLAS_COLUMNS) * GLYPH_W + gx;
    let atlas_y = (glyph_idx / ATLAS_COLUMNS) * GLYPH_H + gy;
    let alpha = textureLoad(glyph_atlas, vec2<i32>(i32(atlas_x), i32(atlas_y)), 0).r;
    return vec4<f32>(1.0, 1.0, 1.0, alpha);
}
