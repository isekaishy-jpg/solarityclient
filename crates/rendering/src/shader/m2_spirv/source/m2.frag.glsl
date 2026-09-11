#version 460

#ifndef M2_SHADOW_FILTERING
#error M2_SHADOW_FILTERING must select direct or comparison shadow sampling
#endif

layout(constant_id = 0) const int M2_PIXEL_EFFECT = 0;
layout(constant_id = 1) const int M2_PIXEL_PERMUTATION = 0;
layout(constant_id = 2) const int M2_TEXTURE_COUNT = 1;

const bool M2_SHADER_ALPHA_TEST = (M2_PIXEL_PERMUTATION / 8) != 0;
const int M2_SHADOW_MODE = M2_PIXEL_PERMUTATION % 4;

struct M2LocalLight {
    vec4 position;
    vec4 ambient;
    vec4 diffuse;
    vec4 attenuation;
};

layout(std140, set = 0, binding = 0) uniform M2SceneState {
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

layout(std140, set = 2, binding = 0) uniform M2MaterialState {
    mat4 model;
    mat4 texture_transforms[2];
    mat4 model_view;
    vec4 mesh_color;
    vec4 fog_color;
    vec4 fragment_parameters;
} material;

layout(set = 3, binding = 0) uniform sampler2D model_texture_0;
layout(set = 3, binding = 1) uniform sampler2D model_texture_1;

#if M2_SHADOW_FILTERING == 0
layout(set = 4, binding = 0) uniform sampler2D shadow_map_0;
layout(set = 4, binding = 1) uniform sampler2D shadow_map_1;
layout(set = 4, binding = 2) uniform sampler2D shadow_map_2;
layout(set = 4, binding = 3) uniform sampler2D shadow_map_3;
#else
layout(set = 4, binding = 0) uniform sampler2DShadow shadow_map_0;
layout(set = 4, binding = 1) uniform sampler2DShadow shadow_map_1;
layout(set = 4, binding = 2) uniform sampler2DShadow shadow_map_2;
layout(set = 4, binding = 3) uniform sampler2DShadow shadow_map_3;
#endif

layout(push_constant) uniform M2DrawState {
    uint bone_transform_offset;
    uint bone_count;
    uint texture_count;
    uint flags;
} draw_state;

layout(location = 0) in vec2 fragment_texture_coordinates_0;
layout(location = 1) in vec2 fragment_texture_coordinates_1;
layout(location = 2) in vec4 fragment_input_color;
layout(location = 3) in float fragment_fog_visibility;
layout(location = 4) in vec3 fragment_view_position;
layout(location = 5) in vec3 fragment_view_normal;
layout(location = 6) in vec3 fragment_shadow_coordinates_0;
layout(location = 7) in vec3 fragment_shadow_coordinates_1;
layout(location = 8) in vec3 fragment_shadow_coordinates_2;
layout(location = 9) in vec3 fragment_shadow_coordinates_3;

layout(location = 0) out vec4 output_color;

// Vulkan comparison samplers reproduce D3D's PCF lookup; direct permutations
// perform the authored sampled-depth-versus-reference comparison explicitly.
#if M2_SHADOW_FILTERING == 0
#define M2_SHADOW_SAMPLER sampler2D
float shadow_sample(M2_SHADOW_SAMPLER map, vec2 coordinates, float reference) {
    return texture(map, coordinates).r >= reference ? 1.0 : 0.0;
}
#else
#define M2_SHADOW_SAMPLER sampler2DShadow
float shadow_sample(M2_SHADOW_SAMPLER map, vec2 coordinates, float reference) {
    return texture(map, vec3(coordinates, reference));
}
#endif

// Stock's short kernel samples the center and odd-numbered c5..c11 offsets.
float shadow_five(M2_SHADOW_SAMPLER map, vec3 coordinates) {
    vec2 base = coordinates.xy * 0.5 + vec2(0.5);
    float visibility = shadow_sample(map, base, coordinates.z);
    visibility += shadow_sample(map, base + scene.shadow_filter_offsets[0].xy, coordinates.z);
    visibility += shadow_sample(map, base + scene.shadow_filter_offsets[2].xy, coordinates.z);
    visibility += shadow_sample(map, base + scene.shadow_filter_offsets[4].xy, coordinates.z);
    visibility += shadow_sample(map, base + scene.shadow_filter_offsets[6].xy, coordinates.z);
    return visibility * 0.2;
}

// The longer kernel adds every recovered c5..c12 offset around the center.
float shadow_nine(M2_SHADOW_SAMPLER map, vec3 coordinates) {
    vec2 base = coordinates.xy * 0.5 + vec2(0.5);
    float visibility = shadow_sample(map, base, coordinates.z);
    for (int offset_index = 0; offset_index < 8; ++offset_index) {
        visibility += shadow_sample(
            map, base + scene.shadow_filter_offsets[offset_index].xy, coordinates.z);
    }
    return visibility / 9.0;
}

// Fade one edge-crossing map back to fully lit exactly as the BLS program does.
float shadow_border(float visibility, vec2 coordinates, float scale, float bias) {
    float weight = clamp(max(abs(coordinates.x), abs(coordinates.y)) * scale + bias, 0.0, 1.0);
    return mix(1.0, visibility, weight);
}

// Combiners_Opaque PS3 uses five taps beyond eye depth 10; our view is right-handed.
float primary_shadow() {
    float edge = clamp(
        max(abs(fragment_shadow_coordinates_0.x), abs(fragment_shadow_coordinates_0.y))
            * -3.4482758 + 3.41379309,
        0.0, 1.0);
    int mode = max(M2_SHADOW_MODE, int(scene.shadow_light_direction.w));
    float visibility = 1.0;
    if (edge > (mode == 3 ? 0.0 : 0.01)) {
        bool short_kernel = mode == 3
            || -fragment_view_position.z > 10.0;
        visibility = short_kernel
            ? shadow_five(shadow_map_0, fragment_shadow_coordinates_0)
            : shadow_nine(shadow_map_0, fragment_shadow_coordinates_0);
        if (mode != 3) {
            visibility = mix(1.0, visibility, edge);
        }
    }
    float plane_fade = clamp(
        dot(fragment_view_position, scene.shadow_fade_plane.xyz)
            + scene.shadow_fade_plane.w,
        0.0, 1.0);
    return mix(visibility, 1.0, plane_fade);
}

// Select the first containing outer map, with the fourth map edge-faded.
float cascade_shadow() {
    if (max(abs(fragment_shadow_coordinates_1.x), abs(fragment_shadow_coordinates_1.y)) < 1.0) {
        return shadow_five(shadow_map_1, fragment_shadow_coordinates_1);
    }
    if (max(abs(fragment_shadow_coordinates_2.x), abs(fragment_shadow_coordinates_2.y)) < 1.0) {
        return shadow_five(shadow_map_2, fragment_shadow_coordinates_2);
    }
    float visibility = shadow_five(shadow_map_3, fragment_shadow_coordinates_3);
    return shadow_border(
        visibility, fragment_shadow_coordinates_3.xy, -11.1111107, 11.0);
}

// Reproduce stock's plane fade, cascade minimum, normal relief, and 70% floor.
float shadow_lighting_factor() {
    if (M2_SHADOW_MODE == 0) {
        return 1.0;
    }
    float visibility = primary_shadow();
    if (max(M2_SHADOW_MODE, int(scene.shadow_light_direction.w)) > 1) {
        visibility = min(visibility, cascade_shadow());
    }
    float facing = 1.2 - abs(dot(scene.shadow_light_direction.xyz, fragment_view_normal));
    float facing_relief = clamp(facing * facing * facing * facing, 0.0, 1.0);
    visibility = mix(visibility, 1.0, facing_relief);
    return visibility * 0.3 + 0.7;
}

// Replay the build-12340 model combiner selected by CM2Shared::GetEffect.
vec4 combine_textures(vec4 texture_0, vec4 texture_1) {
    vec4 input_color = fragment_input_color;
    input_color.rgb *= shadow_lighting_factor();
    bool specular_enabled = scene.fog_parameters.z > 0.5;
    if (M2_PIXEL_EFFECT == 0) {
        return vec4(input_color.rgb * texture_0.rgb, input_color.a);
    }
    if (M2_PIXEL_EFFECT == 1) {
        return vec4(mix(input_color.rgb, texture_0.rgb, input_color.a), input_color.a);
    }
    if (M2_PIXEL_EFFECT == 2) {
        return input_color + texture_0;
    }
    if (M2_PIXEL_EFFECT == 3) {
        return input_color * texture_0 * 2.0;
    }
    if (M2_PIXEL_EFFECT == 4) {
        return vec4(mix(texture_0.rgb, input_color.rgb, input_color.a), input_color.a);
    }
    if (M2_PIXEL_EFFECT == 5) {
        return input_color * texture_0;
    }
    if (M2_PIXEL_EFFECT == 6) {
        return vec4(input_color.rgb * texture_0.rgb * texture_1.rgb, input_color.a);
    }
    if (M2_PIXEL_EFFECT == 7) {
        return vec4(
            input_color.rgb * texture_0.rgb
                + (specular_enabled ? texture_1.rgb : vec3(0.0)),
            input_color.a + texture_1.a);
    }
    if (M2_PIXEL_EFFECT == 8) {
        return vec4(
            input_color.rgb * texture_0.rgb * texture_1.rgb * 2.0,
            input_color.a * texture_1.a * 2.0);
    }
    if (M2_PIXEL_EFFECT == 9) {
        return vec4(
            input_color.rgb * texture_0.rgb * texture_1.rgb * 2.0,
            input_color.a);
    }
    if (M2_PIXEL_EFFECT == 10) {
        return vec4(
            input_color.rgb * texture_0.rgb
                + (specular_enabled ? texture_1.rgb : vec3(0.0)),
            input_color.a);
    }
    if (M2_PIXEL_EFFECT == 11) {
        return vec4(
            input_color.rgb * texture_0.rgb * texture_1.rgb,
            input_color.a * texture_1.a);
    }
    if (M2_PIXEL_EFFECT == 12) {
        return vec4(
            input_color.rgb * texture_0.rgb * texture_1.rgb,
            input_color.a * texture_0.a);
    }
    if (M2_PIXEL_EFFECT == 13) {
        return input_color * texture_0
            + (specular_enabled ? texture_1 : vec4(0.0));
    }
    if (M2_PIXEL_EFFECT == 14) {
        return input_color * texture_0 * texture_1 * 2.0;
    }
    if (M2_PIXEL_EFFECT == 15) {
        return vec4(
            input_color.rgb * texture_0.rgb * texture_1.rgb * 2.0,
            input_color.a * texture_0.a);
    }
    if (M2_PIXEL_EFFECT == 16) {
        return vec4(
            input_color.rgb * texture_0.rgb
                + (specular_enabled ? texture_1.rgb : vec3(0.0)),
            input_color.a * texture_0.a);
    }
    if (M2_PIXEL_EFFECT == 17) {
        return input_color * texture_0 * texture_1;
    }
    if (M2_PIXEL_EFFECT == 18) {
        return (input_color + texture_0) * texture_1;
    }
    if (M2_PIXEL_EFFECT == 19) {
        return input_color * texture_0 * texture_1 * 4.0;
    }
    if (M2_PIXEL_EFFECT == 20) {
        return vec4(
            input_color.rgb * texture_0.rgb
                * mix(texture_1.rgb * 2.0, vec3(1.0), texture_0.a),
            input_color.a);
    }
    if (M2_PIXEL_EFFECT == 21) {
        return vec4(
            input_color.rgb * texture_0.rgb
                + (specular_enabled
                    ? texture_1.rgb * texture_1.a
                    : vec3(0.0)),
            input_color.a);
    }
    if (M2_PIXEL_EFFECT == 22) {
        return vec4(
            input_color.rgb * texture_0.rgb
                + (specular_enabled
                    ? texture_1.rgb * texture_1.a * texture_0.a
                    : vec3(0.0)),
            input_color.a);
    }
    return input_color * texture_0;
}

void main() {
    vec4 texture_0 = texture(model_texture_0, fragment_texture_coordinates_0);
    vec4 texture_1 = M2_TEXTURE_COUNT > 1
        ? texture(model_texture_1, fragment_texture_coordinates_1)
        : vec4(1.0);
    vec4 combined = combine_textures(texture_0, texture_1);
    if (M2_SHADER_ALPHA_TEST && combined.a < material.fragment_parameters.x) {
        discard;
    }

    int fog_mode = int(material.fragment_parameters.y + 0.5);
    vec3 result = combined.rgb;
    if (fog_mode != 0) {
        vec3 fog_color = material.fog_color.rgb;
        if (fog_mode == 2) {
            fog_color = vec3(0.0);
        } else if (fog_mode == 3) {
            fog_color = vec3(1.0);
        } else if (fog_mode == 4) {
            fog_color = vec3(0.5);
        }
        result = mix(fog_color, result, clamp(fragment_fog_visibility, 0.0, 1.0));
    }
    output_color = vec4(result, combined.a);
}
