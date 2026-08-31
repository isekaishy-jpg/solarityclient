#version 460

#ifndef RIBBON_ALPHA_REFERENCE
#error RIBBON_ALPHA_REFERENCE must contain the stock material threshold
#endif

layout(location = 0) in vec2 in_tex_coord;
layout(location = 1) in vec4 in_color;

layout(set = 1, binding = 0) uniform sampler2D ribbon_texture;

layout(location = 0) out vec4 out_color;

void main() {
    vec4 color = texture(ribbon_texture, in_tex_coord) * in_color;
    if (color.a < RIBBON_ALPHA_REFERENCE) {
        discard;
    }
    out_color = color;
}
