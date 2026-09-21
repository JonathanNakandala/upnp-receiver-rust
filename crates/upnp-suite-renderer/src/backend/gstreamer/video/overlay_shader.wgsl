// Same full-viewport-triangle trick as shader.wgsl, duplicated rather than
// shared: it's eight lines, and keeping this shader module self-contained
// avoids threading a second entry point through one bigger file.
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    var out: VertexOutput;
    out.clip_position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return out;
}

// Straight (non-premultiplied) alpha, as GStreamer's plain RGBA video
// caps carry it - the render pipeline's blend state does the actual
// "over" compositing onto the video already drawn underneath.
@group(0) @binding(0) var overlay_texture: texture_2d<f32>;
@group(0) @binding(1) var overlay_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(overlay_texture, overlay_sampler, in.uv);
}
