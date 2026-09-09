#version 460

#ifndef WORLD_MODEL_UNIFIED
#error WORLD_MODEL_UNIFIED must select MapObj or MapObjU
#endif

#ifndef WORLD_MODEL_PRIMARY_SHADOW
#define WORLD_MODEL_PRIMARY_SHADOW 0
#endif
#if WORLD_MODEL_PRIMARY_SHADOW == 1
// Shared with terrain: primary rows act on positions relative to the map origin.
layout(std140, set = 3, binding = 0) uniform WorldShadow {
    vec4 origin_and_texel;
    vec4 receiver_rows[3];
    vec4 light_direction;
} shadow;
layout(location = 6) out vec3 fragment_shadow_coordinates;
layout(location = 7) out vec3 fragment_shadow_normal;
layout(location = 8) out float fragment_eye_depth;
#endif

layout(std140, set = 0, binding = 0) uniform WorldModelSceneState {
    mat4 projection;
    vec4 camera_position;
    vec4 exterior_ambient;
    vec4 exterior_direct;
    vec4 flattened_ambient;
    vec4 flattened_direct;
    vec4 light_direction;
    vec4 fog_parameters;
} scene;

layout(std140, set = 1, binding = 0) uniform WorldModelMaterialState {
    mat4 model;
    vec4 root_ambient;
    vec4 additive_color;
    vec4 fog_color;
    vec4 fragment_parameters;
    uvec4 behavior;
    mat4 model_view;
} material;

layout(location = 0) in vec3 model_position;
layout(location = 1) in vec3 model_normal;
layout(location = 2) in vec2 texture_coordinates_0;
layout(location = 3) in vec2 texture_coordinates_1;
layout(location = 4) in vec4 vertex_color;
layout(location = 5) in vec4 vertex_blend_color;

layout(location = 0) out vec2 fragment_texture_coordinates_0;
layout(location = 1) out vec2 fragment_texture_coordinates_1;
layout(location = 2) out vec2 fragment_reflection_coordinates;
layout(location = 3) out vec4 fragment_color;
layout(location = 4) out vec4 fragment_blend_color;
layout(location = 5) out float fragment_fog_visibility;

void main() {
    vec3 world_position = (material.model * vec4(model_position, 1.0)).xyz;
    vec3 world_normal = normalize(mat3(material.model) * model_normal);
    precise vec4 view_position = material.model_view * vec4(model_position, 1.0);
    gl_Position = scene.projection * view_position;
#if WORLD_MODEL_PRIMARY_SHADOW == 1
    // MapObjDiffuse_T1 variants 30/31 supply position, normalized normal, and
    // c224..226 coordinates. World-space rows preserve the same dot products.
    vec4 relative_position = vec4(world_position - shadow.origin_and_texel.xyz, 1.0);
    fragment_shadow_coordinates = vec3(
        dot(relative_position, shadow.receiver_rows[0]),
        dot(relative_position, shadow.receiver_rows[1]),
        dot(relative_position, shadow.receiver_rows[2]));
    fragment_shadow_normal = world_normal;
    fragment_eye_depth = -view_position.z;
#endif
    fragment_texture_coordinates_0 = texture_coordinates_0;
    fragment_texture_coordinates_1 = texture_coordinates_1;

    // Diffuse_T1_Refl reflects camera-relative position about the transformed
    // normal and passes XY directly to the environment texture stage.
    vec3 camera_relative = world_position - scene.camera_position.xyz;
    vec3 view_direction = length(camera_relative) > 0.00001
        ? normalize(camera_relative)
        : vec3(0.0, 0.0, 1.0);
    fragment_reflection_coordinates = reflect(view_direction, world_normal).xy;

    uint lighting_mode = material.behavior.x;
    bool unlit = material.behavior.y != 0u;
    vec3 ambient = lighting_mode == 2u
        ? scene.flattened_ambient.rgb
        : lighting_mode == 3u
            ? material.root_ambient.rgb
            : scene.exterior_ambient.rgb;
    vec3 direct = lighting_mode == 2u
        ? scene.flattened_direct.rgb
        : lighting_mode == 3u
            ? vec3(0.0)
            : scene.exterior_direct.rgb;
    float diffuse_factor = max(dot(world_normal, normalize(scene.light_direction.xyz)), 0.0);
    vec3 daylight = clamp(ambient + direct * diffuse_factor, vec3(0.0), vec3(1.0));
    vec3 lighting = vertex_color.rgb;
    if (!unlit && lighting_mode != 0u) {
#if WORLD_MODEL_UNIFIED == 1
        lighting = vertex_color.rgb + daylight * 0.5;
#else
        lighting = vertex_color.rgb * daylight;
#endif
        lighting += material.additive_color.rgb;
    }
    fragment_color = vec4(lighting, vertex_color.a);
    fragment_blend_color = vertex_blend_color;

    float fog_start = scene.fog_parameters.x;
    float fog_end = scene.fog_parameters.y;
    float fog_range = max(fog_end - fog_start, 0.001);
    float visibility = clamp((fog_end - length(camera_relative)) / fog_range, 0.0, 1.0);
    fragment_fog_visibility = pow(visibility, max(scene.fog_parameters.w, 0.0));
}
