#version 460

layout(push_constant) uniform RippleFrame {
    mat4 view_projection;
    float depth_bias;
} frame;

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec4 in_color;
layout(location = 2) in vec2 in_coordinates;
layout(location = 0) out vec4 color;
layout(location = 1) out vec2 coordinates;

void main() {
    gl_Position = frame.view_projection * vec4(in_position, 1.0);
    color = in_color;
    coordinates = in_coordinates;
}
