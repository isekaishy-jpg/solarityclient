#version 460

#ifndef TERRAIN_LAYER_COUNT
#error TERRAIN_LAYER_COUNT must be defined
#endif

#if TERRAIN_LAYER_COUNT < 1 || TERRAIN_LAYER_COUNT > 4
#error TERRAIN_LAYER_COUNT lies outside stock's one-through-four range
#endif

layout(location = 0) in vec3 in_normal;
layout(location = 1) in vec2 in_texture_coordinates;
layout(location = 2) in vec2 in_atlas_coordinates;
layout(location = 3) in vec3 in_vertex_light;

layout(set = 0, binding = 0) uniform TerrainScene {
    mat4 view_projection;
    vec4 ambient_color;
    vec4 diffuse_color;
    vec4 sun_direction;
} scene;

layout(set = 1, binding = 0) uniform sampler2D material_atlas;
layout(set = 1, binding = 1) uniform sampler2D diffuse_0;
#if TERRAIN_LAYER_COUNT > 1
layout(set = 1, binding = 2) uniform sampler2D diffuse_1;
#endif
#if TERRAIN_LAYER_COUNT > 2
layout(set = 1, binding = 3) uniform sampler2D diffuse_2;
#endif
#if TERRAIN_LAYER_COUNT > 3
layout(set = 1, binding = 4) uniform sampler2D diffuse_3;
#endif

layout(location = 0) out vec4 out_color;

void main() {
    vec4 material = texture(material_atlas, in_atlas_coordinates);
    vec3 ground = texture(diffuse_0, in_texture_coordinates).rgb;
#if TERRAIN_LAYER_COUNT > 1
    ground = mix(ground, texture(diffuse_1, in_texture_coordinates).rgb, material.r);
#endif
#if TERRAIN_LAYER_COUNT > 2
    ground = mix(ground, texture(diffuse_2, in_texture_coordinates).rgb, material.g);
#endif
#if TERRAIN_LAYER_COUNT > 3
    ground = mix(ground, texture(diffuse_3, in_texture_coordinates).rgb, material.b);
#endif

    vec3 normal = normalize(in_normal);
    vec3 sun = normalize(scene.sun_direction.xyz);
    float diffuse_amount = max(dot(normal, sun), 0.0);
    vec3 lighting = scene.ambient_color.rgb + scene.diffuse_color.rgb * diffuse_amount;
    float baked_shadow = 1.0 - material.a;
    out_color = vec4(ground * lighting * in_vertex_light * baked_shadow, 1.0);
}
