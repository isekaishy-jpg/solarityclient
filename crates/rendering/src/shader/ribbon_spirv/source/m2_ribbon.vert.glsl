#version 460

layout(constant_id = 0) const int RIBBON_WHITE_COLOR = 0;

layout(push_constant) uniform RibbonFog {
    vec4 coefficients;
    vec4 color;
} fog;

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec4 in_color;
layout(location = 2) in vec2 in_tex_coord;

layout(std140, set = 0, binding = 0) uniform M2RibbonScene {
    mat4 projection;
    layout(offset = 784) mat4 view;
} scene;

layout(location = 0) out vec2 out_tex_coord;
layout(location = 1) out vec4 out_color;
layout(location = 2) out float out_fog_visibility;

void main() {
    precise vec4 view_position = scene.view * vec4(in_position, 1.0);
    gl_Position = scene.projection * view_position;
    out_tex_coord = in_tex_coord;
    // 820F40 binds Particle_Unlit. 980B70 selects Color_T1's odd (white)
    // variant for material lighting; its even variant forwards authored PCT.
    out_color = RIBBON_WHITE_COLOR != 0 ? vec4(1.0) : in_color;
    float linear_visibility = max(-view_position.z * fog.coefficients.x + fog.coefficients.y, 0.0);
    // The original program returns one for the initial zero register bank;
    // GLSL leaves pow(0, 0) undefined, so preserve that result explicitly.
    out_fog_visibility = fog.coefficients.z == 0.0 ? 1.0 : min(pow(linear_visibility, fog.coefficients.z), 1.0);
}
