#version 460

#ifndef M2_VERTEX_EFFECT
#error M2_VERTEX_EFFECT must select a stock coordinate effect
#endif
#ifndef M2_VERTEX_PERMUTATION
#error M2_VERTEX_PERMUTATION must select a stock BLS permutation
#endif

const int M2_SHADED = M2_VERTEX_PERMUTATION % 2;
const int M2_LOCAL_LIGHT_COUNT = (M2_VERTEX_PERMUTATION / 2) % 5;
const int M2_BONE_CLASS = (M2_VERTEX_PERMUTATION / 10) % 3;

struct M2LocalLight {
    vec4 position;
    vec4 ambient;
    vec4 diffuse;
    vec4 attenuation;
};

layout(std140, set = 0, binding = 0) uniform M2SceneState {
    mat4 view_projection;
    vec4 camera_position;
    vec4 ambient_light;
    vec4 diffuse_light;
    vec4 light_direction;
    vec4 fog_parameters;
    M2LocalLight local_lights[4];
} scene;

layout(std430, set = 1, binding = 0) readonly buffer M2BoneTransforms {
    mat4 transforms[];
} bones;

layout(std140, set = 2, binding = 0) uniform M2MaterialState {
    mat4 model;
    mat4 texture_transforms[2];
    mat4 environment_view;
    vec4 mesh_color;
    vec4 fog_color;
    vec4 fragment_parameters;
} material;

layout(push_constant) uniform M2DrawState {
    uint bone_transform_offset;
    uint bone_count;
    uint texture_count;
    uint flags;
} draw_state;

layout(location = 0) in vec3 model_position;
layout(location = 1) in vec4 bone_weights;
layout(location = 2) in uvec4 bone_indices;
layout(location = 3) in vec3 model_normal;
layout(location = 4) in vec2 texture_coordinates_0;
layout(location = 5) in vec2 texture_coordinates_1;

layout(location = 0) out vec2 fragment_texture_coordinates_0;
layout(location = 1) out vec2 fragment_texture_coordinates_1;
layout(location = 2) out vec4 fragment_input_color;
layout(location = 3) out float fragment_fog_visibility;

// Build one skin matrix from the exact SKIN influence class selected by stock.
mat4 skin_matrix() {
    if (M2_BONE_CLASS == 0 || draw_state.bone_count == 0u) {
        return mat4(1.0);
    }
    if (M2_BONE_CLASS == 1) {
        return bones.transforms[draw_state.bone_transform_offset + bone_indices.x];
    }

    mat4 skin = mat4(0.0);
    skin += bones.transforms[draw_state.bone_transform_offset + bone_indices.x] * bone_weights.x;
    skin += bones.transforms[draw_state.bone_transform_offset + bone_indices.y] * bone_weights.y;
    skin += bones.transforms[draw_state.bone_transform_offset + bone_indices.z] * bone_weights.z;
    skin += bones.transforms[draw_state.bone_transform_offset + bone_indices.w] * bone_weights.w;
    return skin;
}

// Evaluate the fixed-function-style environment coordinates used by the Env effects.
vec2 environment_coordinates(vec3 position, vec3 normal) {
    vec3 view_position = (material.environment_view * vec4(position, 1.0)).xyz;
    vec3 view_normal = normalize(mat3(material.environment_view) * normal);
    vec3 direction = -normalize(view_position);
    vec3 reflected = direction - 2.0 * view_normal * dot(direction, view_normal);
    reflected.z += 1.0;
    float magnitude = length(reflected);
    return magnitude > 0.00001
        ? normalize(reflected).xy * 0.5 + vec2(0.5)
        : vec2(0.5);
}

// Accumulate stock's saturated sun and bounded local-light terms per vertex.
vec3 lighting(vec3 world_position, vec3 world_normal) {
    if (M2_SHADED == 0) {
        return vec3(1.0);
    }

    vec3 to_sun = normalize(-scene.light_direction.xyz);
    vec3 result = clamp(
        scene.ambient_light.rgb
            + scene.diffuse_light.rgb * max(dot(world_normal, to_sun), 0.0),
        vec3(0.0), vec3(1.0));
    for (int light_index = 0; light_index < M2_LOCAL_LIGHT_COUNT; ++light_index) {
        M2LocalLight light = scene.local_lights[light_index];
        vec3 to_light = light.position.xyz;
        float attenuation = 1.0;
        if (light.position.w > 0.5) {
            to_light -= world_position;
            float distance_squared = dot(to_light, to_light);
            float distance_to_light = sqrt(max(distance_squared, 0.0));
            float denominator = light.attenuation.x
                + light.attenuation.y * distance_to_light
                + light.attenuation.z * distance_squared;
            if (denominator > 0.00001) {
                attenuation = 1.0 / denominator;
            }
        }
        to_light = length(to_light) > 0.00001
            ? normalize(to_light)
            : vec3(0.0, 0.0, 1.0);
        vec3 contribution = (light.ambient.rgb
            + light.diffuse.rgb * max(dot(world_normal, to_light), 0.0)) * attenuation;
        result = clamp(result + contribution, vec3(0.0), vec3(1.0));
    }
    return result;
}

// Apply one authored two-dimensional texture transform without changing the UV domain.
vec2 transform_coordinates(mat4 transform, vec2 coordinates) {
    return (transform * vec4(coordinates, 0.0, 1.0)).xy;
}

void main() {
    mat4 skin = skin_matrix();
    vec3 skinned_position = (skin * vec4(model_position, 1.0)).xyz;
    vec3 skinned_normal = normalize(mat3(skin) * model_normal);
    vec3 world_position = (material.model * vec4(skinned_position, 1.0)).xyz;
    vec3 world_normal = normalize(mat3(material.model) * skinned_normal);
    gl_Position = scene.view_projection * vec4(world_position, 1.0);

    vec2 environment = environment_coordinates(skinned_position, skinned_normal);
    vec2 first = texture_coordinates_0;
    vec2 second = texture_coordinates_1;
    if (M2_VERTEX_EFFECT == 1) {
        first = texture_coordinates_1;
    } else if (M2_VERTEX_EFFECT == 2) {
        first = environment;
    } else if (M2_VERTEX_EFFECT == 4) {
        first = environment;
    } else if (M2_VERTEX_EFFECT == 5) {
        second = environment;
    } else if (M2_VERTEX_EFFECT == 6) {
        first = environment;
        second = environment;
    }
    fragment_texture_coordinates_0 = transform_coordinates(
        material.texture_transforms[0], first);
    fragment_texture_coordinates_1 = transform_coordinates(
        material.texture_transforms[1], second);
    fragment_input_color = clamp(
        material.mesh_color * vec4(lighting(world_position, world_normal), 1.0),
        vec4(0.0), vec4(1.0));

    float fog_start = scene.fog_parameters.x;
    float fog_end = scene.fog_parameters.y;
    float fog_range = max(fog_end - fog_start, 0.001);
    float eye_distance = distance(scene.camera_position.xyz, world_position);
    float linear_visibility = clamp((fog_end - eye_distance) / fog_range, 0.0, 1.0);
    fragment_fog_visibility = pow(
        linear_visibility, max(scene.fog_parameters.w, 0.0));
}
