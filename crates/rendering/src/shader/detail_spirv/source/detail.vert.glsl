#version 460

#ifndef DETAIL_PRIMARY_SHADOW
#define DETAIL_PRIMARY_SHADOW 0
#endif

#if DETAIL_PRIMARY_SHADOW
layout(std140, set = 2, binding = 0) uniform DetailShadow {
    vec4 origin_and_texel;
    vec4 receiver_rows[3];
    vec4 light_direction;
    vec4 environment_rows[9];
    vec4 fade_plane;
    vec4 settings;
} shadow;
layout(location = 4) out vec3 shadow_coordinates;
layout(location = 5) out vec3 shadow_normal;
layout(location = 6) out vec3 environment_coordinates[3];
layout(location = 9) out float shadow_eye_depth;
#endif

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec4 in_color;
layout(location = 3) in vec2 in_coordinates;

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

layout(push_constant) uniform DetailDraw {
    vec4 origin_and_distance;
    vec4 shadow_origin;
} draw;

layout(location = 0) out vec4 color;
layout(location = 1) out vec2 coordinates;
layout(location = 2) out float fog_visibility;
layout(location = 3) out float detail_visibility;

void main() {
    // 7B1B50 retains chunk-local geometry. DetailDoodad.bls applies the
    // translated model-view first and projection second.
    precise vec3 translated_origin = mat3(scene.view) * draw.origin_and_distance.xyz;
    precise vec4 view_position = vec4(mat3(scene.view) * in_position + translated_origin, 1.0);
    gl_Position = scene.projection * view_position;
    float depth = -view_position.z;
    float range = draw.origin_and_distance.w;
    // Native 7B15D0 starts the detail alpha ramp at 85% of groundEffectDist.
    detail_visibility = clamp((range - depth) / (range - range * 0.85), 0.0, 1.0);
    float fog = max((scene.fog_parameters.y - depth)
        / (scene.fog_parameters.y - scene.fog_parameters.x), 0.0);
    fog_visibility = scene.fog_color.w > 0.0
        ? min(pow(fog, scene.fog_parameters.w), 1.0) : 1.0;
    float diffuse = clamp(dot(in_normal, scene.sun_direction.xyz), 0.0, 1.0);
    color.rgb = min(scene.ambient_color.rgb + scene.diffuse_color.rgb * diffuse, vec3(1.0)) * in_color.rgb;
    color.a = in_color.a;
    coordinates = in_coordinates;
#if DETAIL_PRIMARY_SHADOW
    // DetailDoodad.bls variant one applies the receiver rows to view-space
    // vertices. Equivalent world-space rows retain chunk-local precision here.
    vec4 relative_position = vec4(in_position + draw.shadow_origin.xyz, 1.0);
    shadow_coordinates = vec3(dot(relative_position, shadow.receiver_rows[0]),
        dot(relative_position, shadow.receiver_rows[1]),
        dot(relative_position, shadow.receiver_rows[2]));
    for (int map = 0; map < 3; ++map) {
        environment_coordinates[map] = vec3(
            dot(relative_position, shadow.environment_rows[map * 3]),
            dot(relative_position, shadow.environment_rows[map * 3 + 1]),
            dot(relative_position, shadow.environment_rows[map * 3 + 2]));
    }
    shadow_eye_depth = depth;
    // Native relief uses the interpolated terrain normal without normalization.
    shadow_normal = in_normal;
#endif
}
