#version 460

layout(push_constant) uniform UnderwaterFrame {
    mat4 projection;
    vec4 fog_parameters;
    vec4 fog_color;
} frame;

layout(set = 0, binding = 0) uniform sampler2D surface;
layout(location = 0) in vec4 color;
layout(location = 1) in vec2 coordinates;
layout(location = 2) in float fog_visibility;
layout(location = 0) out vec4 out_color;

void main() {
    out_color = texture(surface, coordinates) * color;
    // Native GX blend 2 selects alpha reference 1 with GREATER_OR_EQUAL.
    if (out_color.a < (1.0 / 255.0)) {
        discard;
    }
    out_color.rgb = mix(frame.fog_color.rgb, out_color.rgb, fog_visibility);
}
