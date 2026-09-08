#version 460

layout(push_constant) uniform UnderwaterFrame {
    mat4 projection;
    vec4 fog_parameters;
    vec4 fog_color;
} frame;
layout(location = 0) in vec3 in_position;
layout(location = 1) in vec4 in_color;
layout(location = 2) in vec2 in_coordinates;
layout(location = 0) out vec4 color;
layout(location = 1) out vec2 coordinates;
layout(location = 2) out float fog_visibility;

void main() {
    gl_Position = frame.projection * vec4(in_position, 1.0);
    color = in_color;
    coordinates = in_coordinates;
    // 6A3A60 enables linear vertex fog; range fog remains disabled.
    // Native input is already in positive-Z view space.
    fog_visibility = clamp(in_position.z * frame.fog_parameters.x + frame.fog_parameters.y, 0.0, 1.0);
}
