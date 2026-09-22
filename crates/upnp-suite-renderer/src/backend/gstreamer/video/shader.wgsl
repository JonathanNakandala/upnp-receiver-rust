// A real quad (6 vertices, 2 triangles, `renderer.rs`'s `QUAD_VERTICES`) -
// not the single oversized-triangle-from-vertex_index trick this used to
// be. That trick only works because its "extends past the clip volume"
// vertices are at fixed positions with no per-vertex data; Keystone needs
// each corner warped independently (see `keystone_scale` below), which
// only makes sense for vertices actually placed at the quad's real
// corners - so this went back to a plain vertex buffer once Keystone
// needed real per-corner positions to warp.
struct VertexInput {
    @location(0) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// UPnP's HorizontalKeystone/VerticalKeystone, as a trapezoidal warp of the
// quad's corners - not a fragment effect, a vertex position change. Per the
// spec: negative Horizontal shrinks the left edge (left/right edges stay
// parallel, left just gets shorter toward a point as the value approaches
// its minimum); positive shrinks the right edge the same way. Vertical is
// the same shape rotated 90 degrees (negative shrinks the bottom, positive
// the top). `pos` is the *other* axis's coordinate in [0, 1] (`uv.x` for
// the horizontal case, `uv.y` for the vertical case) - that's what "which
// edge" means geometrically: the edge at `pos = 0` shrinks under
// `shrink_near`, the edge at `pos = 1` under `shrink_far`, and interpolates
// linearly between them. `shrink_near`/`shrink_far` are each in [0, 1] (one
// of them is always 0 - a single keystone value only ever shrinks one
// edge), so the result stays in [0, 1] too: this only ever shrinks toward
// the quad's center, never expands past its original bounds, which is why
// it composes cleanly with `letterboxed_viewport`'s aspect-correct
// viewport rect without needing to know about it.
fn keystone_scale(pos: f32, shrink_near: f32, shrink_far: f32) -> f32 {
    return 1.0 - shrink_near * (1.0 - pos) - shrink_far * pos;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let uv = in.uv;

    let h_shrink_left = max(-render_params.horizontal_keystone, 0.0);
    let h_shrink_right = max(render_params.horizontal_keystone, 0.0);
    let y_scale = keystone_scale(uv.x, h_shrink_left, h_shrink_right);

    let v_shrink_bottom = max(-render_params.vertical_keystone, 0.0);
    let v_shrink_top = max(render_params.vertical_keystone, 0.0);
    let x_scale = keystone_scale(uv.y, v_shrink_bottom, v_shrink_top);

    // Both computed from the same original `uv`, not chained one into the
    // other - keeps the two axes independent regardless of which combination
    // of Horizontal/Vertical values is active at once.
    let warped = vec2<f32>(0.5 + (uv.x - 0.5) * x_scale, 0.5 + (uv.y - 0.5) * y_scale);

    var out: VertexOutput;
    out.clip_position = vec4<f32>(warped * 2.0 - 1.0, 0.0, 1.0);
    // Flip Y: clip space has +Y up, texture space has +Y down. Uses the
    // *original* `uv`, not `warped` - a warped quad's corners still map to
    // the source image's plain corners; only their on-screen position
    // moved, which is exactly what stretches the sampled image into the
    // trapezoid shape.
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
    // Keystone: see the vertex shader section below - these are read there,
    // not here, but declared in the same uniform block as everything else.
    horizontal_keystone: f32,
    vertical_keystone: f32,
    // Sharpness: read in `fs_main`, see `sharpen_luma` below for why this
    // is luma-only and unlike every other field here, 0 (not 50) is the
    // spec's own neutral/minimum.
    sharpness_amount: f32,
}
@group(1) @binding(0) var<uniform> render_params: RenderParams;

// UPnP's `Sharpness` - unsharp masking (`sharpened = original + amount *
// (original - blurred)`), the standard sharpening technique used across
// video/photo processing (TV "Sharpness" controls, ffmpeg's `unsharp`,
// Photoshop's Sharpen filter). Implemented here as the discrete 5-tap
// Laplacian kernel (`4*center - N - S - E - W`), the textbook single-pass
// approximation of unsharp masking when a separate blur pass isn't
// affordable - exactly this renderer's situation, one fragment shader
// invocation per pixel, no intermediate render target.
//
// Luma-only, deliberately: real hardware sharpening (TVs, video
// processors) never touches chroma - chroma already carries a fraction of
// luma's spatial resolution (this renderer's own NV12/P010 4:2:0 chroma
// plane is half resolution in both dimensions), and sharpening it produces
// visible color fringing around edges rather than perceived detail, since
// the human eye's sharpness perception is almost entirely luma-driven
// anyway. `uv_sample` (chroma) is untouched by this.
//
// `amount = 0` (UPnP's own minimum, and this renderer's neutral point -
// unlike every other RenderParams field, where neutral sits at the
// midpoint of its range) means `sharpened == y_sample` exactly: the
// Laplacian term is still computed but multiplied by zero, so this is a
// true no-op at the spec's minimum, not just "close to unsharpened."
fn sharpen_luma(uv: vec2<f32>, amount: f32) -> f32 {
    let dims = vec2<f32>(textureDimensions(y_texture));
    let texel = 1.0 / dims;

    let center = textureSample(y_texture, frame_sampler, uv).r;
    let north = textureSample(y_texture, frame_sampler, uv - vec2<f32>(0.0, texel.y)).r;
    let south = textureSample(y_texture, frame_sampler, uv + vec2<f32>(0.0, texel.y)).r;
    let west = textureSample(y_texture, frame_sampler, uv - vec2<f32>(texel.x, 0.0)).r;
    let east = textureSample(y_texture, frame_sampler, uv + vec2<f32>(texel.x, 0.0)).r;

    let laplacian = center * 4.0 - north - south - east - west;
    return center + amount * laplacian;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let y_sample = sharpen_luma(in.uv, render_params.sharpness_amount);
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
