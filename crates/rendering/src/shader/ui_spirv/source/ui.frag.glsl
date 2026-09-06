#version 460

layout(location = 0) in vec2 in_tex_coord;
layout(location = 1) in vec4 in_color;
#if UI_MASKED
layout(location = 2) in vec2 in_mask_coord;
layout(set = 1, binding = 0) uniform sampler2D ui_mask;
#endif

#if UI_TEXTURED
layout(set = 0, binding = 0) uniform sampler2D ui_texture;
#endif

layout(location = 0) out vec4 out_color;

void main() {
#if UI_TEXTURED
    out_color = texture(ui_texture, in_tex_coord) * in_color;
#if UI_MASKED
    out_color.a *= texture(ui_mask, in_mask_coord).a;
#endif
#else
    out_color = in_color;
#endif
}
