#version 460

#ifndef WORLD_MODEL_SHADER
#error WORLD_MODEL_SHADER must select a normalized MOMT effect
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
    vec3 result = diffuse * fragment_color.rgb * 2.0 + emissive;

    uint fog_mode = material.behavior.z;
    if (fog_mode != 0u) {
        vec3 fog = material.fog_color.rgb;
        if (fog_mode == 2u) {
            fog = vec3(0.0);
        } else if (fog_mode == 3u) {
            fog = vec3(1.0);
        } else if (fog_mode == 4u) {
            fog = vec3(0.5);
        }
        result = mix(fog, result, clamp(fragment_fog_visibility, 0.0, 1.0));
    }
    output_color = vec4(result, surface_alpha);
}
