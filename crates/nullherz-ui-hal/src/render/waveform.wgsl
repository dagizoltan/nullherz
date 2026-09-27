struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct Globals {
    scroll_offset: f32,
    zoom: f32,
    is_vertical: u32,
    waveform_style: u32,
    accent_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;

    let pos_x = (model.position.x - globals.scroll_offset) * globals.zoom - 1.0;
    if (globals.is_vertical != 0u) {
        // Vertical orientation: amplitude on X (-1..1), time on Y (-1..1)
        out.clip_position = vec4<f32>(model.position.y, pos_x, 0.0, 1.0);
    } else {
        // Horizontal orientation: time on X (-1..1), amplitude on Y (-1..1)
        out.clip_position = vec4<f32>(pos_x, model.position.y, 0.0, 1.0);
    }

    out.color = model.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
