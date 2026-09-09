#version 460

#ifndef TERRAIN_PRIMARY_SHADOW
#define TERRAIN_PRIMARY_SHADOW 0
#endif

#if TERRAIN_PRIMARY_SHADOW
layout(set = 2, binding = 0) uniform TerrainShadow {
    vec4 origin_and_texel;
    vec4 receiver_rows[3];
} shadow;
layout(location = 5) out vec3 out_shadow_coordinates;
#endif

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec2 in_texture_coordinates;
layout(location = 3) in vec2 in_alpha_coordinates;
layout(location = 4) in vec3 in_color_rgb;

layout(set = 0, binding = 0) uniform TerrainScene {
    mat4 projection;
    vec4 ambient_color;
    vec4 diffuse_color;
    vec4 sun_direction;
    vec4 view_depth;
    vec4 fog_parameters;
    vec4 fog_color;
    mat4 view;
} scene;

layout(push_constant) uniform TerrainDraw {
    uvec2 atlas_chunk;
} draw;

layout(location = 1) out vec2 out_texture_coordinates;
layout(location = 2) out vec2 out_atlas_coordinates;
layout(location = 3) out vec3 out_vertex_light;
layout(location = 4) out float out_fog_visibility;

void main() {
    const float chunk_texels = 64.0;
    const float atlas_texels = 1024.0;
    vec2 atlas_origin = vec2(draw.atlas_chunk) * chunk_texels;

    // Terrain.bls uses c0..c3 for view, then c4..c7 for projection. Keeping
    // clip Z separate prevents depth cancellation at large world coordinates.
    precise vec4 view_position = scene.view * vec4(in_position, 1.0);
    gl_Position = scene.projection * view_position;
    // Original Terrain.bls uses view Z and c12, then pow/max/min into oFog.
    float eye_depth = -dot(scene.view_depth, vec4(in_position, 1.0));
    float inverse_range = 1.0 / (scene.fog_parameters.y - scene.fog_parameters.x);
    float visibility = max((scene.fog_parameters.y - eye_depth) * inverse_range, 0.0);
    out_fog_visibility = scene.fog_color.w > 0.0
        ? min(pow(visibility, scene.fog_parameters.w), 1.0) : 1.0;
    out_texture_coordinates = in_texture_coordinates;
    out_atlas_coordinates = (atlas_origin + in_alpha_coordinates * chunk_texels) / atlas_texels;

    // Terrain.bls lights vertices before interpolation. The normal and light
    // direction are transformed by the same camera rotation in the original.
    float diffuse_amount = clamp(dot(in_normal, scene.sun_direction.xyz), 0.0, 1.0);
    vec3 lighting = min(scene.ambient_color.rgb + scene.diffuse_color.rgb * diffuse_amount, vec3(1.0));
    out_vertex_light = lighting * in_color_rgb;
#if TERRAIN_PRIMARY_SHADOW
    vec4 relative_position = vec4(in_position - shadow.origin_and_texel.xyz, 1.0);
    out_shadow_coordinates = vec3(
        dot(relative_position, shadow.receiver_rows[0]),
        dot(relative_position, shadow.receiver_rows[1]),
        dot(relative_position, shadow.receiver_rows[2]));
#endif
}
