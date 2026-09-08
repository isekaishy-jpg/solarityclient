#version 460
layout(set = 0, binding = 0) uniform sampler2D cloud_texture;
layout(location = 0) in vec4 color;
layout(location = 1) in vec2 uv;
layout(location = 0) out vec4 out_color;
void main() { out_color = color * texture(cloud_texture, uv); }
