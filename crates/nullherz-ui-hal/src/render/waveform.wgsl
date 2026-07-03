struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct Globals {
    scroll_offset: f32,
    zoom: f32,
    accent_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;

    // Apply horizontal scroll and zoom
    let x = (model.position.x - globals.scroll_offset) * globals.zoom;
    out.clip_position = vec4<f32>(x, model.position.y, 0.0, 1.0);

    // Simple color based on height for now
    out.color = globals.accent_color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
