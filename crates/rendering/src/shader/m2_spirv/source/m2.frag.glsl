#version 460

#ifndef M2_PIXEL_EFFECT
#error M2_PIXEL_EFFECT must select a stock combiner effect
#endif
#ifndef M2_PIXEL_PERMUTATION
#error M2_PIXEL_PERMUTATION must select a stock BLS permutation
#endif

const bool M2_SHADER_ALPHA_TEST = (M2_PIXEL_PERMUTATION / 8) != 0;

layout(std140, set = 0, binding = 0) uniform M2SceneState {
    mat4 view_projection;
    vec4 camera_position;
    vec4 ambient_light;
    vec4 diffuse_light;
    vec4 light_direction;
    vec4 fog_parameters;
} scene;

layout(std140, set = 2, binding = 0) uniform M2MaterialState {
    mat4 model;
    mat4 texture_transforms[2];
    mat4 environment_view;
    vec4 mesh_color;
    vec4 fog_color;
    vec4 fragment_parameters;
} material;

layout(set = 3, binding = 0) uniform sampler2D model_texture_0;
layout(set = 3, binding = 1) uniform sampler2D model_texture_1;

layout(push_constant) uniform M2DrawState {
    uint bone_start;
    uint bone_count;
    uint texture_count;
    uint flags;
} draw_state;

layout(location = 0) in vec2 fragment_texture_coordinates_0;
layout(location = 1) in vec2 fragment_texture_coordinates_1;
layout(location = 2) in vec4 fragment_input_color;
layout(location = 3) in float fragment_fog_visibility;

layout(location = 0) out vec4 output_color;

// Replay the build-12340 model combiner selected by CM2Shared::GetEffect.
vec4 combine_textures(vec4 texture_0, vec4 texture_1) {
    vec4 input_color = fragment_input_color;
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
            input_color.rgb * texture_0.rgb + texture_1.rgb,
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
            input_color.rgb * texture_0.rgb + texture_1.rgb,
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
        return input_color * texture_0 + texture_1;
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
            input_color.rgb * texture_0.rgb + texture_1.rgb,
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
            input_color.rgb * texture_0.rgb + texture_1.rgb * texture_1.a,
            input_color.a);
    }
    if (M2_PIXEL_EFFECT == 22) {
        return vec4(
            input_color.rgb * texture_0.rgb
                + texture_1.rgb * texture_1.a * texture_0.a,
            input_color.a);
    }
    return input_color * texture_0;
}

void main() {
    vec4 texture_0 = texture(model_texture_0, fragment_texture_coordinates_0);
    vec4 texture_1 = draw_state.texture_count > 1u
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
