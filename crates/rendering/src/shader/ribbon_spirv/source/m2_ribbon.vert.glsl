#version 460

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec4 in_color;
layout(location = 2) in vec2 in_tex_coord;

layout(std140, set = 0, binding = 0) uniform M2RibbonScene {
    mat4 projection;
    layout(offset = 784) mat4 view;
} scene;

layout(location = 0) out vec2 out_tex_coord;
layout(location = 1) out vec4 out_color;

void main() {
    precise vec4 view_position = scene.view * vec4(in_position, 1.0);
    gl_Position = scene.projection * view_position;
    out_tex_coord = in_tex_coord;
    out_color = in_color;
}
