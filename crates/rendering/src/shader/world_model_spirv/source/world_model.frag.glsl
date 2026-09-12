#version 460

#ifndef WORLD_MODEL_SHADER
#error WORLD_MODEL_SHADER must select a normalized MOMT effect
#endif

#ifndef WORLD_MODEL_PRIMARY_SHADOW
#define WORLD_MODEL_PRIMARY_SHADOW 0
#endif
#if WORLD_MODEL_PRIMARY_SHADOW == 1
layout(std140, set = 3, binding = 0) uniform WorldShadow {
    vec4 origin_and_texel;
    vec4 receiver_rows[3];
    vec4 light_direction;
    vec4 environment_rows[9];
    vec4 fade_plane;
    vec4 settings;
} shadow;
layout(set = 3, binding = 1) uniform sampler2D primary_shadow_map;
layout(set = 3, binding = 2) uniform sampler2D environment_shadow_0;
layout(set = 3, binding = 3) uniform sampler2D environment_shadow_1;
layout(set = 3, binding = 4) uniform sampler2D environment_shadow_2;
layout(location = 6) in vec3 fragment_shadow_coordinates;
layout(location = 7) in vec3 fragment_shadow_normal;
layout(location = 8) in float fragment_eye_depth;
layout(location = 9) in vec3 environment_coordinates[3];

// The direct-depth kernels use c3..10 (terrain) or c5..12 (MapObj/detail).
float filtered_shadow(sampler2D map, vec3 coordinates, int step_size) {
    const vec2 offsets[8] = vec2[8](
        vec2(0.8, -1.0), vec2(-0.2, -0.8), vec2(0.2, -0.6), vec2(1.0, -0.4),
        vec2(-0.6, -0.2), vec2(0.6, 0.2), vec2(-1.0, -0.4), vec2(-0.4, -0.6));
    vec2 uv = coordinates.xy * 0.5 + vec2(0.5);
    float visibility = float(textureLod(map, uv, 0.0).r >= coordinates.z);
    for (int index = 0; index < 8; index += step_size) {
        visibility += float(textureLod(map,
            uv + offsets[index] * shadow.origin_and_texel.w, 0.0).r >= coordinates.z);
    }
    return visibility / (step_size == 2 ? 5.0 : 9.0);
}

// Original PS3 selects the first containing environment map and fades the far edge.
float environment_shadow_visibility() {
    if (max(abs(environment_coordinates[0].x), abs(environment_coordinates[0].y)) < 1.0) {
        return filtered_shadow(environment_shadow_0, environment_coordinates[0], 2);
    }
    if (max(abs(environment_coordinates[1].x), abs(environment_coordinates[1].y)) < 1.0) {
        return filtered_shadow(environment_shadow_1, environment_coordinates[1], 2);
    }
    float visibility = filtered_shadow(environment_shadow_2, environment_coordinates[2], 2);
    float edge = clamp(max(abs(environment_coordinates[2].x), abs(environment_coordinates[2].y))
        * -11.1111107 + 11.0, 0.0, 1.0);
    return mix(1.0, visibility, edge);
}

// MapObj PS2 retains primary edge fading and eye-depth filtering; PS3 uses
// five taps with no primary edge fade. Both take the environment-map minimum.
float primary_shadow_factor() {
    int mode = int(shadow.settings.x);
    float edge = clamp(max(abs(fragment_shadow_coordinates.x), abs(fragment_shadow_coordinates.y))
        * -3.4482758 + 3.41379309, 0.0, 1.0);
    float visibility = 1.0;
    if (edge > (mode == 3 ? 0.0 : 0.01)) {
        int step_size = mode == 3 || fragment_eye_depth > 10.0 ? 2 : 1;
        visibility = filtered_shadow(primary_shadow_map, fragment_shadow_coordinates, step_size);
        if (mode != 3) {
            visibility = min(mix(1.0, visibility, edge), 1.0);
        }
    }
    if (mode > 1) {
        visibility = min(visibility, environment_shadow_visibility());
    }
    // Stock interpolates the vertex-normalized normal without normalizing again.
    float facing = 1.2 - abs(dot(shadow.light_direction.xyz, fragment_shadow_normal));
    float squared = facing * facing;
    visibility = mix(visibility, 1.0, clamp(squared * squared, 0.0, 1.0));
    return 0.7 + 0.3 * visibility;
}
#endif

layout(std140, set = 1, binding = 0) uniform WorldModelMaterialState {
    mat4 model;
    vec4 root_ambient;
    vec4 additive_color;
    vec4 fog_color;
    vec4 fragment_parameters;
    uvec4 behavior;
    mat4 model_view;
} material;

layout(set = 2, binding = 0) uniform sampler2D material_texture_0;
layout(set = 2, binding = 1) uniform sampler2D material_texture_1;

layout(location = 0) in vec2 fragment_texture_coordinates_0;
layout(location = 1) in vec2 fragment_texture_coordinates_1;
layout(location = 2) in vec2 fragment_reflection_coordinates;
layout(location = 3) in vec4 fragment_color;
layout(location = 4) in vec4 fragment_blend_color;
layout(location = 5) in float fragment_fog_visibility;
layout(location = 0) out vec4 output_color;

void main() {
    vec4 primary = texture(material_texture_0, fragment_texture_coordinates_0);
    vec4 secondary = vec4(0.0);
#if WORLD_MODEL_SHADER == 3 || WORLD_MODEL_SHADER == 5
    secondary = texture(material_texture_1, fragment_reflection_coordinates);
#elif WORLD_MODEL_SHADER == 6
    secondary = texture(material_texture_1, fragment_texture_coordinates_1);
#endif

    vec3 diffuse = primary.rgb;
    vec3 emissive = vec3(0.0);
    float opacity = primary.a * fragment_color.a;
#if WORLD_MODEL_SHADER == 1 || WORLD_MODEL_SHADER == 2 || WORLD_MODEL_SHADER == 4
    opacity = fragment_color.a;
#elif WORLD_MODEL_SHADER == 3
    emissive = secondary.rgb * primary.a;
    opacity = fragment_color.a;
#elif WORLD_MODEL_SHADER == 5
    emissive = primary.rgb * primary.a * secondary.rgb;
    opacity = fragment_color.a;
#elif WORLD_MODEL_SHADER == 6
    vec4 composite = mix(secondary, primary, fragment_blend_color.a);
    diffuse = composite.rgb;
    opacity = composite.a * fragment_color.a;
#endif

    // Opaque has a zero reference and performs no coverage test. Every other
    // direct blend uses the fixed threshold supplied from the GX table.
    float alpha_reference = material.fragment_parameters.x;
    if (alpha_reference > 0.0 && opacity < alpha_reference) {
        discard;
    }
    float surface_alpha = material.behavior.w != 0u ? 1.0 : opacity;
    vec3 lighting = fragment_color.rgb;
#if WORLD_MODEL_PRIMARY_SHADOW == 1
    lighting *= primary_shadow_factor();
#endif
    vec3 result = diffuse * lighting * 2.0 + emissive;

    uint fog_mode = material.behavior.z;
    if (fog_mode != 0u) {
        vec3 fog = material.fog_color.rgb;
        result = mix(fog, result, clamp(fragment_fog_visibility, 0.0, 1.0));
    }
    output_color = vec4(result, surface_alpha);
}
