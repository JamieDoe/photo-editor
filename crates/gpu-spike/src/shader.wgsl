// The render plan as one fused compute pass. Mirrors renderer::ops exactly (no LUTs),
// so results are compared against the CPU backend within quantisation tolerance.

struct Params {
    gains: vec4<f32>,
    contrast_gamma: f32,
    saturation: f32,
    flags: u32,        // bit 0: contrast, bit 1: saturation, bit 2: base curve
    pixel_count: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
// Interleaved RGB u16 samples, two per u32 word.
@group(0) @binding(1) var<storage, read> src: array<u32>;
// RGBA8 packed per pixel.
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;

const WORKGROUP: u32 = 256u;
const MID_GREY: f32 = 0.18;
const PERCEPTUAL_GAMMA: f32 = 2.2;

fn sample(i: u32) -> f32 {
    let word = src[i >> 1u];
    let v = select(word >> 16u, word & 0xffffu, (i & 1u) == 0u);
    return f32(v) / 65535.0;
}

fn contrast(x: f32, g: f32) -> f32 {
    if (x < 0.0 || x >= 1.0) { return max(x, 0.0); }
    let p = pow(x, 1.0 / PERCEPTUAL_GAMMA);
    let m = pow(MID_GREY, 1.0 / PERCEPTUAL_GAMMA);
    var q: f32;
    if (p <= m) { q = m * pow(p / m, g); } else { q = 1.0 - (1.0 - m) * pow((1.0 - p) / (1.0 - m), g); }
    return pow(q, PERCEPTUAL_GAMMA);
}

// Standard look (renderer::ops::look::standard): log-logistic S-curve after a lift.
const LOOK_LIFT: f32 = 2.3784142;
const LOOK_SLOPE: f32 = 1.65;
const LOOK_PIVOT: f32 = 0.55;

fn log_logistic(v: f32) -> f32 {
    return 1.0 / (1.0 + pow(LOOK_PIVOT / v, LOOK_SLOPE));
}

fn base_curve(x: f32) -> f32 {
    if (x <= 0.0) { return 0.0; }
    if (x >= 1.0) { return x; }
    return log_logistic(x * LOOK_LIFT) / log_logistic(LOOK_LIFT);
}

fn encode(x: f32) -> u32 {
    let v = clamp(x, 0.0, 1.0);
    let s = select(1.055 * pow(v, 1.0 / 2.4) - 0.055, v * 12.92, v <= 0.0031308);
    return u32(round(s * 255.0));
}

@compute @workgroup_size(256)
fn main(@builtin(workgroup_id) wg: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>,
        @builtin(local_invocation_index) li: u32) {
    let idx = (wg.y * nwg.x + wg.x) * WORKGROUP + li;
    if (idx >= params.pixel_count) { return; }
    var rgb = vec3<f32>(sample(idx * 3u), sample(idx * 3u + 1u), sample(idx * 3u + 2u)) * params.gains.xyz;
    if ((params.flags & 1u) != 0u) {
        rgb = vec3<f32>(contrast(rgb.x, params.contrast_gamma), contrast(rgb.y, params.contrast_gamma),
                        contrast(rgb.z, params.contrast_gamma));
    }
    if ((params.flags & 4u) != 0u) {
        rgb = vec3<f32>(base_curve(rgb.x), base_curve(rgb.y), base_curve(rgb.z));
    }
    if ((params.flags & 2u) != 0u) {
        let y = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        rgb = max(vec3<f32>(y) + (rgb - vec3<f32>(y)) * params.saturation, vec3<f32>(0.0));
    }
    dst[idx] = encode(rgb.x) | (encode(rgb.y) << 8u) | (encode(rgb.z) << 16u) | (255u << 24u);
}
