// Full-viewport triangle: three vertices generated purely from
// `vertex_index`, no vertex buffer needed. Standard trick - the triangle
// extends past the clip volume on two corners, which gets clipped away,
// leaving exactly the viewport covered.
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    var out: VertexOutput;
    out.clip_position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    // Flip Y: clip space has +Y up, texture space has +Y down.
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return out;
}

// Luma (R8Unorm/R16Unorm) and interleaved chroma (Rg8Unorm/Rg16Unorm) planes
// as decoded (NV12 or P010) - never converted to RGB on the CPU. Sampling
// either format normalizes to the same [0, 1] fraction of that format's
// value range, so this conversion is bit-depth-agnostic: an 8-bit sample at
// 128/255 and a 10-bit sample at 512/1023 (stored as 512<<6 in a 16-bit
// P010 word) both read back as ~0.502, so the same math below is correct
// for both without knowing which one it's sampling.
@group(0) @binding(0) var y_texture: texture_2d<f32>;
@group(0) @binding(1) var uv_texture: texture_2d<f32>;
@group(0) @binding(2) var frame_sampler: sampler;

// UPnP RenderingControl's picture state, precomputed in `renderer.rs` from
// the UPnP-facing values (0-100 for Brightness/Contrast/ColorTemperature,
// 0-1000 for the Gain/BlackLevel pair - see `video::RenderParams`'s doc for
// why) into the multipliers/offsets this shader actually applies, so this
// shader stays agnostic of those scales.
//
// Composition order below is deliberate, in three stages, each applied to
// the previous stage's output:
//   1. gain/black_level - per-channel calibration (`Output = Gain * Input +
//      BlackLevel`, the standard 2-point white balance model), correcting
//      the signal toward accurate.
//   2. color_temperature - a warm/cool tint relative to that corrected
//      signal, still a per-channel multiply.
//   3. contrast/brightness - the final global picture-preference layer, on
//      top of an already color-corrected signal.
// All three happen after YUV -> RGB, and deliberately the very last
// operations in this shader - see this crate's RenderingControl design
// notes on why: if HDR/wide-gamut output is ever added, whatever
// tone-maps it down to this same display-referred RGB must run *before*
// this whole block, never after, or the contrast pivot-at-0.5 math (tuned
// for gamma-encoded SDR output) stops meaning what a control point expects
// "more contrast" to mean.
struct RenderParams {
    red_gain: f32,
    green_gain: f32,
    blue_gain: f32,
    red_black_level: f32,
    green_black_level: f32,
    blue_black_level: f32,
    temp_red: f32,
    temp_green: f32,
    temp_blue: f32,
    contrast_factor: f32,
    brightness_offset: f32,
}
@group(1) @binding(0) var<uniform> render_params: RenderParams;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let y_sample = textureSample(y_texture, frame_sampler, in.uv).r;
    let uv_sample = textureSample(uv_texture, frame_sampler, in.uv).rg;

    // BT.709, studio/limited range (the common case for compressed video:
    // luma in [16, 235], chroma in [16, 240] of a notional 8-bit scale,
    // expressed here as fractions so it applies unchanged at any bit
    // depth). Not colorimetry-aware yet - BT.601 sources and full-range
    // (JPEG-style) sources will be slightly off until that's read from the
    // stream's caps.
    let y = y_sample - 16.0 / 255.0;
    let u = uv_sample.x - 128.0 / 255.0;
    let v = uv_sample.y - 128.0 / 255.0;

    let r = 1.164384 * y + 1.792741 * v;
    let g = 1.164384 * y - 0.213249 * u - 0.532909 * v;
    let b = 1.164384 * y + 2.112402 * u;

    let gain = vec3<f32>(render_params.red_gain, render_params.green_gain, render_params.blue_gain);
    let black_level = vec3<f32>(
        render_params.red_black_level,
        render_params.green_black_level,
        render_params.blue_black_level,
    );
    let calibrated = vec3<f32>(r, g, b) * gain + black_level;

    let temp = vec3<f32>(render_params.temp_red, render_params.temp_green, render_params.temp_blue);
    let tinted = calibrated * temp;

    let adjusted = (tinted - 0.5) * render_params.contrast_factor
        + 0.5 + render_params.brightness_offset;
    return vec4<f32>(clamp(adjusted, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
