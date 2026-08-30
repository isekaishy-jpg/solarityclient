#version 460

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_tex_coord;
layout(location = 2) in vec4 in_color;

layout(location = 0) out vec2 out_tex_coord;
layout(location = 1) out vec4 out_color;

layout(push_constant) uniform UiCanvas {
    vec2 logical_extent;
} ui_canvas;

void main() {
    vec2 clip = (in_position / ui_canvas.logical_extent) * 2.0 - 1.0;
    gl_Position = vec4(clip, 0.0, 1.0);
    out_tex_coord = in_tex_coord;
    out_color = in_color;
}
