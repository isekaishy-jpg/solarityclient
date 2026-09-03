#version 460

layout(constant_id = 0) const int PARTICLE_SHADED = 0;

struct M2LocalLight {
    vec4 position;
    vec4 ambient;
    vec4 diffuse;
    vec4 attenuation;
};

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec4 in_color;
layout(location = 3) in vec2 in_tex_coord;

layout(std140, set = 0, binding = 0) uniform M2ParticleScene {
    mat4 view_projection;
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
} scene;

layout(location = 0) out vec2 out_tex_coord;
layout(location = 1) out vec4 out_color;
layout(location = 2) out float out_fog_visibility;

vec3 lighting(vec3 position, vec3 normal) {
    if (PARTICLE_SHADED == 0) {
        return vec3(1.0);
    }
    normal = length(normal) > 0.00001 ? normalize(normal) : vec3(0.0, 0.0, 1.0);
    vec3 to_sun = normalize(-scene.light_direction.xyz);
    vec3 result = scene.ambient_light.rgb
        + scene.diffuse_light.rgb * max(dot(normal, to_sun), 0.0);
    for (int light_index = 0; light_index < 4; ++light_index) {
        M2LocalLight light = scene.local_lights[light_index];
        vec3 to_light = light.position.xyz;
        float attenuation = 1.0;
        if (light.position.w > 0.5) {
            to_light -= position;
            float distance_squared = dot(to_light, to_light);
            float distance_to_light = sqrt(max(distance_squared, 0.0));
            float denominator = light.attenuation.x
                + light.attenuation.y * distance_to_light
                + light.attenuation.z * distance_squared;
            attenuation = denominator > 0.00001 ? 1.0 / denominator : 1.0;
        }
        if (length(to_light) > 0.00001) {
            to_light = normalize(to_light);
        }
        result += (light.ambient.rgb
            + light.diffuse.rgb * max(dot(normal, to_light), 0.0)) * attenuation;
    }
    return clamp(result, vec3(0.0), vec3(1.0));
}

void main() {
    gl_Position = scene.view_projection * vec4(in_position, 1.0);
    out_tex_coord = in_tex_coord;
    out_color = vec4(in_color.rgb * lighting(in_position, in_normal), in_color.a);
    float fog_range = max(scene.fog_parameters.y - scene.fog_parameters.x, 0.001);
    float camera_distance = distance(scene.camera_position.xyz, in_position);
    out_fog_visibility = clamp(
        (scene.fog_parameters.y - camera_distance) / fog_range, 0.0, 1.0);
}
