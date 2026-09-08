#version 460
layout(push_constant) uniform CloudFrame { mat4 view_projection; } frame;
layout(location = 0) in vec3 in_position;
layout(location = 1) in vec4 in_color;
layout(location = 2) in vec2 in_uv;
layout(location = 0) out vec4 color;
layout(location = 1) out vec2 uv;
void main() {
    gl_Position = frame.view_projection * vec4(in_position, 1.0);
    gl_Position.z = gl_Position.w * 0.9990234375 + gl_Position.z * 0.0009765625;
    color = in_color;
    uv = in_uv;
}
