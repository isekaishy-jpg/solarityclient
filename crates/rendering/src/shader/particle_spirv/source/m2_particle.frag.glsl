#version 460

layout(constant_id = 0) const float PARTICLE_ALPHA_REFERENCE = 0.0;
layout(constant_id = 1) const int PARTICLE_FOG_MODE = 0;

struct M2LocalLight {
    vec4 position;
    vec4 ambient;
    vec4 diffuse;
    vec4 attenuation;
};

layout(std140, set = 0, binding = 0) uniform M2ParticleScene {
    mat4 projection;
    vec4 camera_position;
    vec4 ambient_light;
    vec4 diffuse_light;
    vec4 light_direction;
    vec4 fog_parameters;
    vec4 fog_color;
    M2LocalLight local_lights[4];
    vec4 shadow_matrix_rows[12];
    vec4 shadow_fade_plane;
    vec4 shadow_light_direction;
    vec4 shadow_filter_offsets[8];
    vec4 view_depth_plane;
    mat4 view;
} scene;

layout(location = 0) in vec2 in_tex_coord;
layout(location = 1) in vec4 in_color;
layout(location = 2) in float in_fog_visibility;

layout(set = 1, binding = 0) uniform sampler2D particle_texture;

layout(location = 0) out vec4 out_color;

void main() {
    vec4 color = texture(particle_texture, in_tex_coord) * in_color;
    if (color.a < PARTICLE_ALPHA_REFERENCE) {
        discard;
    }
    vec3 result = color.rgb;
    if (PARTICLE_FOG_MODE != 0) {
        vec3 fog_color = scene.fog_color.rgb;
        if (PARTICLE_FOG_MODE == 2) {
            fog_color = vec3(0.0);
        } else if (PARTICLE_FOG_MODE == 3) {
            fog_color = vec3(1.0);
        }
        result = mix(fog_color, result, in_fog_visibility);
    }
    out_color = vec4(result, color.a);
}
