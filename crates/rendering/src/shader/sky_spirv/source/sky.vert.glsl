#version 460

layout(push_constant) uniform SkyFrame { mat4 view_projection; } frame;
layout(location = 0) in vec3 in_position;
layout(location = 1) in vec4 in_color;
layout(location = 0) out vec4 color;

void main() {
    gl_Position = frame.view_projection * vec4(in_position, 1.0);
    // 7F09B0 reserves the final 1/1024 of the depth range for the sky pass.
    gl_Position.z = gl_Position.w * 0.9990234375 + gl_Position.z * 0.0009765625;
    color = in_color;
}
