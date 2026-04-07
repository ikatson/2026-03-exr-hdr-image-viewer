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
    sdr_white_vs_input: f32, // on windows, this is > 1. means we need to scale the output at the end so that 1.0 = 80 nits
    peak_luma_vs_sdr_white: f32, // peak luma relative to 1. (SDR white). For peak luma computations.
    tone_map_mode: u32, // 0: passthrough, 1: ACES, 2: Reno ACES, 3: GT7, 4: Reinhard, 5: Neutwo
    is_yuv: u32,
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

const RENO_ACES_MIN_STOP_SDR: f32 = -6.5;
const RENO_ACES_MAX_STOP_SDR: f32 = 6.5;
const RENO_ACES_MIN_STOP_RRT: f32 = -15.0;
const RENO_ACES_MAX_STOP_RRT: f32 = 18.0;
const RENO_ACES_MIN_LUM_SDR: f32 = 0.02;
const RENO_ACES_MAX_LUM_SDR: f32 = 48.0;
const RENO_ACES_MIN_LUM_RRT: f32 = 0.0001;
const RENO_ACES_MAX_LUM_RRT: f32 = 10000.0;
const RENO_ACES_LIM_CYAN: f32 = 1.147;
const RENO_ACES_LIM_MAGENTA: f32 = 1.264;
const RENO_ACES_LIM_YELLOW: f32 = 1.312;
const RENO_ACES_THR_CYAN: f32 = 0.815;
const RENO_ACES_THR_MAGENTA: f32 = 0.803;
const RENO_ACES_THR_YELLOW: f32 = 0.880;
const RENO_ACES_GAMUT_PWR: f32 = 1.2;
const RENO_REFERENCE_WHITE_NITS: f32 = 240.0;

fn reno_bt709_to_ap1(c: vec3<f32>) -> vec3<f32> {
    return mul_rows3(
        vec3<f32>(0.6130974024, 0.3395231462, 0.0473794514),
        vec3<f32>(0.0701937225, 0.9163538791, 0.0134523985),
        vec3<f32>(0.0206155929, 0.1095697729, 0.8698146342),
        c,
    );
}

fn reno_ap1_to_ap0(c: vec3<f32>) -> vec3<f32> {
    return mul_rows3(
        vec3<f32>(0.6954522414, 0.1406786965, 0.1638690622),
        vec3<f32>(0.0447945634, 0.8596711185, 0.0955343182),
        vec3<f32>(-0.0055258826, 0.0040252103, 1.0015006723),
        c,
    );
}

fn reno_ap0_to_ap1(c: vec3<f32>) -> vec3<f32> {
    return mul_rows3(
        vec3<f32>(1.4514393161, -0.2365107469, -0.2149285693),
        vec3<f32>(-0.0765537734, 1.1762296998, -0.0996759264),
        vec3<f32>(0.0083161484, -0.0060324498, 0.9977163014),
        c,
    );
}

fn reno_ap1_to_bt709(c: vec3<f32>) -> vec3<f32> {
    return mul_rows3(
        vec3<f32>(1.7050509927, -0.6217921207, -0.0832588720),
        vec3<f32>(-0.1302564175, 1.1408047366, -0.0105483191),
        vec3<f32>(-0.0240033568, -0.1289689761, 1.1529723329),
        c,
    );
}

fn reno_ap1_rgb2y(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2722287168, 0.6740817658, 0.0536895174));
}

fn reno_rgb2yc(rgb: vec3<f32>) -> f32 {
    let chroma = sqrt(
        rgb.b * (rgb.b - rgb.g) +
        rgb.g * (rgb.g - rgb.r) +
        rgb.r * (rgb.r - rgb.b)
    );
    return (rgb.r + rgb.g + rgb.b + 1.75 * chroma) / 3.0;
}

fn reno_rgb2saturation(rgb: vec3<f32>) -> f32 {
    let minrgb = min(min(rgb.r, rgb.g), rgb.b);
    let maxrgb = max(max(rgb.r, rgb.g), rgb.b);
    return (max(maxrgb, 1e-10) - max(minrgb, 1e-10)) / max(maxrgb, 1e-2);
}

fn reno_sigmoid_shaper(x: f32) -> f32 {
    let t = max(1.0 - abs(0.5 * x), 0.0);
    let y = 1.0 + sign(x) * (1.0 - t * t);
    return 0.5 * y;
}

fn reno_glow_fwd(yc_in: f32, glow_gain_in: f32, glow_mid: f32) -> f32 {
    if yc_in <= (2.0 / 3.0) * glow_mid {
        return glow_gain_in;
    }
    if yc_in >= 2.0 * glow_mid {
        return 0.0;
    }
    return glow_gain_in * (glow_mid / yc_in - 0.5);
}

fn reno_rgb2hue(rgb: vec3<f32>) -> f32 {
    if rgb.r == rgb.g && rgb.g == rgb.b {
        return 0.0;
    }
    var hue =
        degrees(atan2(sqrt(3.0) * (rgb.g - rgb.b), 2.0 * rgb.r - rgb.g - rgb.b));
    if hue < 0.0 {
        hue = hue + 360.0;
    }
    return clamp(hue, 0.0, 360.0);
}

fn reno_center_hue(hue: f32, center_h: f32) -> f32 {
    var hue_centered = hue - center_h;
    if hue_centered < -180.0 {
        hue_centered = hue_centered + 360.0;
    } else if hue_centered > 180.0 {
        hue_centered = hue_centered - 360.0;
    }
    return hue_centered;
}

fn reno_interpolate_1d(p0: vec2<f32>, p1: vec2<f32>, p: f32) -> f32 {
    if p < p0.x {
        return p0.y;
    }
    if p >= p1.x {
        return p1.y;
    }
    let s = (p - p0.x) / (p1.x - p0.x);
    return p0.y * (1.0 - s) + p1.y * s;
}

fn reno_lookup_aces_min(min_lum_log10: f32) -> f32 {
    let v = reno_interpolate_1d(
        vec2<f32>(log10f(RENO_ACES_MIN_LUM_RRT), RENO_ACES_MIN_STOP_RRT),
        vec2<f32>(log10f(RENO_ACES_MIN_LUM_SDR), RENO_ACES_MIN_STOP_SDR),
        min_lum_log10,
    );
    return 0.18 * exp2(v);
}

fn reno_lookup_aces_max(max_lum_log10: f32) -> f32 {
    let v = reno_interpolate_1d(
        vec2<f32>(log10f(RENO_ACES_MAX_LUM_SDR), RENO_ACES_MAX_STOP_SDR),
        vec2<f32>(log10f(RENO_ACES_MAX_LUM_RRT), RENO_ACES_MAX_STOP_RRT),
        max_lum_log10,
    );
    return 0.18 * exp2(v);
}

struct RenoOdtConfig {
    y_min: vec3<f32>,
    y_mid: vec3<f32>,
    y_max: vec3<f32>,
    coefs_low: array<f32, 6>,
    coefs_high: array<f32, 6>,
}

fn reno_create_odt_config(min_y: f32, max_y: f32) -> RenoOdtConfig {
    var config: RenoOdtConfig;
    let min_lum_log10 = log10f(min_y);
    let max_lum_log10 = log10f(max_y);
    let aces_min = reno_lookup_aces_min(min_lum_log10);
    let aces_max = reno_lookup_aces_max(max_lum_log10);
    let mid_pt = vec3<f32>(0.18, 4.8, 1.55);
    let log_min = vec2<f32>(log10f(aces_min), min_lum_log10);
    let log_mid = vec2<f32>(log10f(mid_pt.x), log10f(mid_pt.y));
    let log_max = vec2<f32>(log10f(aces_max), max_lum_log10);

    let knot_inc_low = (log_mid.x - log_min.x) / 3.0;
    config.coefs_low[0] = log_min.y;
    config.coefs_low[1] = log_min.y;
    let min_coef = log_mid.y - mid_pt.z * log_mid.x;
    config.coefs_low[3] = mid_pt.z * (log_mid.x - 0.5 * knot_inc_low) + min_coef;
    config.coefs_low[4] = mid_pt.z * (log_mid.x + 0.5 * knot_inc_low) + min_coef;
    config.coefs_low[5] = config.coefs_low[4];
    let pct_low = reno_interpolate_1d(
        vec2<f32>(RENO_ACES_MIN_STOP_RRT, 0.18),
        vec2<f32>(RENO_ACES_MIN_STOP_SDR, 0.35),
        log2(aces_min / 0.18),
    );
    config.coefs_low[2] = log_min.y + pct_low * (log_mid.y - log_min.y);

    let knot_inc_high = (log_max.x - log_mid.x) / 3.0;
    config.coefs_high[0] = mid_pt.z * (log_mid.x - 0.5 * knot_inc_high) + min_coef;
    config.coefs_high[1] = mid_pt.z * (log_mid.x + 0.5 * knot_inc_high) + min_coef;
    config.coefs_high[3] = log_max.y;
    config.coefs_high[4] = log_max.y;
    config.coefs_high[5] = log_max.y;
    let pct_high = reno_interpolate_1d(
        vec2<f32>(RENO_ACES_MAX_STOP_SDR, 0.89),
        vec2<f32>(RENO_ACES_MAX_STOP_RRT, 0.90),
        log2(aces_max / 0.18),
    );
    config.coefs_high[2] = log_mid.y + pct_high * (log_max.y - log_mid.y);

    config.y_min = vec3<f32>(log_min.x, log_min.y, 0.0);
    config.y_mid = vec3<f32>(log_mid.x, log_mid.y, mid_pt.z);
    config.y_max = vec3<f32>(log_max.x, log_max.y, 0.0);
    return config;
}

fn reno_spline_eval(c0: f32, c1: f32, c2: f32, t: f32) -> f32 {
    let mcf = vec3<f32>(
        0.5 * c0 - c1 + 0.5 * c2,
        -c0 + c1,
        0.5 * c0 + 0.5 * c1,
    );
    return dot(vec3<f32>(t * t, t, 1.0), mcf);
}

fn reno_ssts(x: f32, config: RenoOdtConfig) -> f32 {
    let log_x = log10f(max(x, 1e-10));
    var log_y: f32;

    if log_x > config.y_max.x {
        log_y = config.y_max.y;
    } else if log_x >= config.y_mid.x {
        let knot_coord = 3.0 * (log_x - config.y_mid.x) / (config.y_max.x - config.y_mid.x);
        let j = min(i32(knot_coord), 2);
        let t = knot_coord - f32(j);
        log_y = reno_spline_eval(
            config.coefs_high[j],
            config.coefs_high[j + 1],
            config.coefs_high[j + 2],
            t,
        );
    } else if log_x > config.y_min.x {
        let knot_coord = 3.0 * (log_x - config.y_min.x) / (config.y_mid.x - config.y_min.x);
        let j = min(i32(knot_coord), 2);
        let t = knot_coord - f32(j);
        log_y = reno_spline_eval(
            config.coefs_low[j],
            config.coefs_low[j + 1],
            config.coefs_low[j + 2],
            t,
        );
    } else {
        log_y = config.y_min.y;
    }

    return pow(10.0, log_y);
}

fn reno_gamut_compress_channel(dist: f32, lim: f32, thr: f32, pwr: f32) -> f32 {
    if dist < thr {
        return dist;
    }
    let scl =
        (lim - thr) /
        pow(pow((1.0 - thr) / (lim - thr), -pwr) - 1.0, 1.0 / pwr);
    let nd = (dist - thr) / scl;
    let p = pow(nd, pwr);
    return thr + scl * nd / pow(1.0 + p, 1.0 / pwr);
}

fn reno_gamut_compress(lin_ap1: vec3<f32>) -> vec3<f32> {
    let ach = max(lin_ap1.r, max(lin_ap1.g, lin_ap1.b));
    let abs_ach = abs(ach);
    var dist = vec3<f32>(0.0);
    if ach != 0.0 {
        dist = (vec3<f32>(ach) - lin_ap1) / abs_ach;
    }
    let compr_dist = vec3<f32>(
        reno_gamut_compress_channel(dist.r, RENO_ACES_LIM_CYAN, RENO_ACES_THR_CYAN, RENO_ACES_GAMUT_PWR),
        reno_gamut_compress_channel(dist.g, RENO_ACES_LIM_MAGENTA, RENO_ACES_THR_MAGENTA, RENO_ACES_GAMUT_PWR),
        reno_gamut_compress_channel(dist.b, RENO_ACES_LIM_YELLOW, RENO_ACES_THR_YELLOW, RENO_ACES_GAMUT_PWR),
    );
    return vec3<f32>(ach) - compr_dist * abs_ach;
}

fn reno_rrt(aces: vec3<f32>) -> vec3<f32> {
    var c = aces;
    let saturation = reno_rgb2saturation(c);
    let yc_in = reno_rgb2yc(c);
    let s = reno_sigmoid_shaper((saturation - 0.4) / 0.2);
    let added_glow = 1.0 + reno_glow_fwd(yc_in, 0.05 * s, 0.08);
    c = c * added_glow;

    let hue = reno_rgb2hue(c);
    let centered_hue = reno_center_hue(hue, 0.0);
    var hue_weight = smoothstep(0.0, 1.0, 1.0 - abs(2.0 * centered_hue / 135.0));
    hue_weight = hue_weight * hue_weight;
    c.r = c.r + hue_weight * saturation * (0.03 - c.r) * (1.0 - 0.82);

    c = clamp(c, vec3<f32>(0.0), vec3<f32>(65535.0));
    var rgb_pre = reno_ap0_to_ap1(c);
    rgb_pre = clamp(rgb_pre, vec3<f32>(0.0), vec3<f32>(65504.0));
    rgb_pre = mix(vec3<f32>(reno_ap1_rgb2y(rgb_pre)), rgb_pre, 0.96);
    return rgb_pre;
}

fn reno_odt_tonemap(rgb: vec3<f32>, min_y: f32, max_y: f32) -> vec3<f32> {
    let config = reno_create_odt_config(min_y, max_y);
    return clamp(
        vec3<f32>(
            reno_ssts(rgb.r, config),
            reno_ssts(rgb.g, config),
            reno_ssts(rgb.b, config),
        ),
        vec3<f32>(0.0),
        vec3<f32>(65535.0),
    );
}

fn reno_odt(rgb_pre: vec3<f32>, min_y: f32, max_y: f32) -> vec3<f32> {
    let tonescaled = reno_odt_tonemap(rgb_pre, min_y, max_y);
    return reno_ap1_to_bt709(tonescaled);
}

fn reno_aces_hdr(color: vec3<f32>, peak: f32) -> vec3<f32> {
    let min_y = 0.0001;
    let safe_peak = max(peak, 1.0);
    let max_y = safe_peak * RENO_REFERENCE_WHITE_NITS;
    let config = reno_create_odt_config(min_y, max_y);

    var c = reno_bt709_to_ap1(max(color, vec3<f32>(0.0)) * RENO_REFERENCE_WHITE_NITS);
    c = reno_gamut_compress(c);
    c = reno_ap1_to_ap0(c);
    c = reno_rrt(c);
    c = clamp(
        vec3<f32>(
            reno_ssts(c.r, config),
            reno_ssts(c.g, config),
            reno_ssts(c.b, config),
        ),
        vec3<f32>(0.0),
        vec3<f32>(65535.0),
    );
    c = reno_ap1_to_bt709(c);

    let sdr_white_out = reno_ssts(RENO_REFERENCE_WHITE_NITS, config);
    return max(c / sdr_white_out, vec3<f32>(0.0));
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

fn agx_default_contrast_approx(x: vec3<f32>) -> vec3<f32> {
    let x2 = x * x;
    let x4 = x2 * x2;
    let x6 = x4 * x2;
    return  - 17.86     * x6 * x
            + 78.01     * x6
            - 126.7     * x4 * x
            + 92.06     * x4
            - 28.72     * x2 * x
            + 4.361     * x2
            - 0.1718    * x
            + 0.002857;
}

fn agx_tonemap(color: vec3<f32>, peak: f32) -> vec3<f32> {
    let agx_mat = mat3x3<f32>(
        vec3<f32>( 0.842479062253094,  0.0423282422610123, 0.0423756549057051),
        vec3<f32>( 0.0784335999999992, 0.878468636469772,  0.0784336          ),
        vec3<f32>( 0.0792237451477643, 0.0791661274605434, 0.879142973793104  ),
    );

    let agx_mat_inv = mat3x3<f32>(
        vec3<f32>( 1.19687900512017,   -0.0528968517574562, -0.0529716355144438),
        vec3<f32>(-0.0980208811401368,  1.15190312990417,   -0.0980434501171241),
        vec3<f32>(-0.0990297440797205, -0.0989611768448433,  1.15107367264116  ),
    );

    let min_ev = -12.47393;
    let max_ev =   4.026069;

    var c = color;

    c = agx_mat * c;
    c = clamp(log2(max(c, vec3<f32>(1e-10))), vec3<f32>(min_ev), vec3<f32>(max_ev));
    c = (c - min_ev) / (max_ev - min_ev);
    c = agx_default_contrast_approx(c);
    c = agx_mat_inv * c;

    // Scale to display range: 1.0 = SDR white, peak = display max luminance
    return max(c, vec3<f32>(0.0)) * peak;
}

fn tonemap(color: vec3<f32>, tone_map_mode: u32, peak: f32) -> vec3<f32> {
    if tone_map_mode == 0u {
        return color;
    }
    if tone_map_mode == 1u {
        return neutwo_tonemap(color, peak);
    }
    if tone_map_mode == 2u {
        return gt7_tonemap(color, peak);
    }
    if tone_map_mode == 3u {
        return reinhard_tonemap(color);
    }
    if tone_map_mode == 4u {
        return aces_hdr(color, peak);
    }
    if tone_map_mode == 5u {
        return reno_aces_hdr(color, peak);
    }
    if tone_map_mode == 6u {
        return agx_tonemap(color, peak);
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

fn pq_eotf(c: vec3<f32>) -> vec3<f32> {
    // [0-1] non-lilnear (PQ) rgb -> [0-10000] absolute nits
    // You can divide by e.g. 203 (HDR reference white per bt2100) to normalize to 1.0 as SDR max
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
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
    let r = sample_channel(r_tex, uv);
    let g = sample_channel(g_tex, uv);
    let b = sample_channel(b_tex, uv);
    var color = vec3<f32>(r, g, b);
    if params.is_yuv == 1 {
        // [0-65535] -> [0-1] rgb
        color = convert_yuv_to_rgb(color);

        color = apply_bt2390_eetf(color, 0.1, 500);

        // [0-1] rgb -> [0-1] linear, where 1 is 10000 nits
        color = pq_eotf(color);
        // normalize to SDR max white (so that 1. == SDR max). At least on OSX this is fine.
        // on windows TBD
        color = color / 300.;

        // output is linear rec 709 on Windows and seemingy on OSX too
        color = bt2020_to_709(color);
    }
    color *= params.exposure;
    color = tonemap(color, params.tone_map_mode, params.peak_luma_vs_sdr_white) * params.sdr_white_vs_input;
    return vec4<f32>(color, 1.0);
}
