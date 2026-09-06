#version 460

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_tex_coord;
layout(location = 2) in vec4 in_color;

layout(location = 0) out vec2 out_tex_coord;
layout(location = 1) out vec4 out_color;
#if UI_MASKED
layout(location = 2) out vec2 out_mask_coord;
#endif

layout(push_constant) uniform UiCanvas {
    vec2 logical_extent;
    vec2 translation;
    float opacity;
#if UI_MASKED
    layout(offset = 20) float mask_left;
    float mask_top;
    float mask_inverse_width;
    float mask_inverse_height;
#endif
} ui_canvas;

void main() {
    vec2 clip = ((in_position + ui_canvas.translation) / ui_canvas.logical_extent) * 2.0 - 1.0;
    gl_Position = vec4(clip, 0.0, 1.0);
    out_tex_coord = in_tex_coord;
    out_color = vec4(in_color.rgb, in_color.a * ui_canvas.opacity);
#if UI_MASKED
    out_mask_coord = vec2(
        (in_position.x - ui_canvas.mask_left) * ui_canvas.mask_inverse_width,
        (ui_canvas.mask_top - in_position.y) * ui_canvas.mask_inverse_height);
#endif
}
