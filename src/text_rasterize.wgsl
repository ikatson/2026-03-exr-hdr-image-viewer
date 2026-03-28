@group(0) @binding(0)
var<storage, read> picked_color: array<vec4<f32>, 1>;

@group(0) @binding(1)
var out_tex: texture_storage_2d<rgba8unorm, write>;

fn digit_char(d: u32) -> u32 {
    return 48u + min(d, 9u);
}

fn char_at(index: u32, r: f32, g: f32, b: f32) -> u32 {
    let channels = array<f32, 3>(max(r, 0.0), max(g, 0.0), max(b, 0.0));
    let chars_per_channel = 13u;
    let channel_idx = select(
        2u,
        select(1u, 0u, index < chars_per_channel),
        index < chars_per_channel * 2u,
    );
    let local = index - channel_idx * chars_per_channel;
    let v = channels[channel_idx];
    let int_part = min(u32(v), 999999u);
    let frac_rounded = min(u32(fract(v) * 1000.0 + 0.5), 999u);
    let i6 = (int_part / 100000u) % 10u;
    let i5 = (int_part / 10000u) % 10u;
    let i4 = (int_part / 1000u) % 10u;
    let i3 = (int_part / 100u) % 10u;
    let i2 = (int_part / 10u) % 10u;
    let i1 = int_part % 10u;
    let f1 = (frac_rounded / 100u) % 10u;
    let f2 = (frac_rounded / 10u) % 10u;
    let f3 = frac_rounded % 10u;

    if local == 0u {
        return select(82u, select(71u, 66u, channel_idx == 1u), channel_idx == 0u);
    }
    if local == 1u {
        return 58u; // :
    }
    if local == 2u {
        return digit_char(i6);
    }
    if local == 3u {
        return digit_char(i5);
    }
    if local == 4u {
        return digit_char(i4);
    }
    if local == 5u {
        return digit_char(i3);
    }
    if local == 6u {
        return digit_char(i2);
    }
    if local == 7u {
        return digit_char(i1);
    }
    if local == 8u {
        return 46u; // .
    }
    if local == 9u {
        return digit_char(f1);
    }
    if local == 10u {
        return digit_char(f2);
    }
    if local == 11u {
        return digit_char(f3);
    }
    return 32u; // space separator
}

fn glyph_row(ch: u32, row: u32) -> u32 {
    if row > 6u {
        return 0u;
    }

    if ch == 32u {
        return 0u;
    }
    if ch == 46u {
        if row == 6u {
            return 4u;
        }
        return 0u;
    }
    if ch == 58u {
        if row == 2u || row == 4u {
            return 4u;
        }
        return 0u;
    }
    if ch == 66u {
        let rows = array<u32, 7>(
            30u,
            17u,
            17u,
            30u,
            17u,
            17u,
            30u,
        );
        return rows[row];
    }
    if ch == 71u {
        let rows = array<u32, 7>(
            14u,
            17u,
            16u,
            23u,
            17u,
            17u,
            15u,
        );
        return rows[row];
    }
    if ch == 82u {
        let rows = array<u32, 7>(
            30u,
            17u,
            17u,
            30u,
            20u,
            18u,
            17u,
        );
        return rows[row];
    }

    if ch == 48u {
        let rows = array<u32, 7>(
            14u,
            17u,
            17u,
            17u,
            17u,
            17u,
            14u,
        );
        return rows[row];
    }
    if ch == 49u {
        let rows = array<u32, 7>(
            4u,
            12u,
            4u,
            4u,
            4u,
            4u,
            14u,
        );
        return rows[row];
    }
    if ch == 50u {
        let rows = array<u32, 7>(
            30u,
            1u,
            1u,
            30u,
            16u,
            16u,
            31u,
        );
        return rows[row];
    }
    if ch == 51u {
        let rows = array<u32, 7>(
            30u,
            1u,
            1u,
            14u,
            1u,
            1u,
            30u,
        );
        return rows[row];
    }
    if ch == 52u {
        let rows = array<u32, 7>(
            18u,
            18u,
            18u,
            31u,
            2u,
            2u,
            2u,
        );
        return rows[row];
    }
    if ch == 53u {
        let rows = array<u32, 7>(
            31u,
            16u,
            16u,
            30u,
            1u,
            1u,
            30u,
        );
        return rows[row];
    }
    if ch == 54u {
        let rows = array<u32, 7>(
            14u,
            16u,
            16u,
            30u,
            17u,
            17u,
            14u,
        );
        return rows[row];
    }
    if ch == 55u {
        let rows = array<u32, 7>(
            31u,
            1u,
            2u,
            4u,
            8u,
            8u,
            8u,
        );
        return rows[row];
    }
    if ch == 56u {
        let rows = array<u32, 7>(
            14u,
            17u,
            17u,
            14u,
            17u,
            17u,
            14u,
        );
        return rows[row];
    }
    if ch == 57u {
        let rows = array<u32, 7>(
            14u,
            17u,
            17u,
            15u,
            1u,
            1u,
            14u,
        );
        return rows[row];
    }
    return 0u;
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(out_tex);
    if gid.x >= dims.x || gid.y >= dims.y {
        return;
    }

    let r = picked_color[0].x;
    let g = picked_color[0].y;
    let b = picked_color[0].z;

    let origin = vec2<u32>(16u, 16u);
    let char_w = 28u;
    let char_h = 40u;
    let glyph_w = 5u;
    let glyph_h = 7u;
    let text_chars = 39u;
    let text_w = text_chars * char_w;

    var color = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    if gid.x >= origin.x && gid.x < origin.x + text_w && gid.y >= origin.y && gid.y < origin.y + char_h {
        let local = vec2<u32>(gid.x - origin.x, gid.y - origin.y);
        let char_idx = local.x / char_w;
        let char_local_x = local.x % char_w;

        let gap_x = 3u;
        if char_local_x >= char_w - gap_x {
            textureStore(out_tex, vec2<i32>(gid.xy), color);
            return;
        }

        let draw_w = char_w - gap_x;
        let base_scale = min(draw_w / glyph_w, char_h / glyph_h);
        let scale = max(1u, base_scale - 1u);
        let glyph_px_w = glyph_w * scale;
        let glyph_px_h = glyph_h * scale;
        let pad_x = (draw_w - glyph_px_w) / 2u;
        let pad_y = (char_h - glyph_px_h) / 2u;
        if char_local_x >= pad_x && char_local_x < pad_x + glyph_px_w && local.y >= pad_y && local.y < pad_y + glyph_px_h {
            let gx = (char_local_x - pad_x) / scale;
            let gy = (local.y - pad_y) / scale;
            let c = char_at(char_idx, r, g, b);
            let row_bits = glyph_row(c, gy);
            let bit = (row_bits >> (glyph_w - 1u - gx)) & 1u;
            if bit == 1u {
                color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
            }
        }
    }

    textureStore(out_tex, vec2<i32>(gid.xy), color);
}
