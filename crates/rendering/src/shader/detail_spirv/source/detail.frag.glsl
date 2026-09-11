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
layout(set = 2, binding = 1) uniform sampler2D primary_shadow_map;
layout(set = 2, binding = 2) uniform sampler2D environment_shadow_0;
layout(set = 2, binding = 3) uniform sampler2D environment_shadow_1;
layout(set = 2, binding = 4) uniform sampler2D environment_shadow_2;
layout(location = 4) in vec3 shadow_coordinates;
layout(location = 5) in vec3 shadow_normal;
layout(location = 6) in vec3 environment_coordinates[3];
layout(location = 9) in float shadow_eye_depth;
#endif

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
layout(set = 1, binding = 0) uniform sampler2D surface;
layout(location = 0) in vec4 color;
layout(location = 1) in vec2 coordinates;
layout(location = 2) in float fog_visibility;
layout(location = 3) in float detail_visibility;
layout(location = 0) out vec4 out_color;

#if DETAIL_PRIMARY_SHADOW
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

// DetailDoodad PS2_0 variant one retains baked visibility. PS3_0 variants
// two/three omit it and follow MapObj's respective cached/cascaded filtering.
float primary_shadow_visibility(float baked_visibility) {
    int mode = int(shadow.settings.x);
    float edge = clamp(max(abs(shadow_coordinates.x), abs(shadow_coordinates.y))
        * -3.4482758 + 3.41379309, 0.0, 1.0);
    float visibility = mode > 1 ? 1.0 : baked_visibility;
    if (edge > (mode == 2 ? 0.01 : 0.0)) {
        int step_size = mode != 2 || shadow_eye_depth > 10.0 ? 2 : 1;
        visibility = filtered_shadow(primary_shadow_map, shadow_coordinates, step_size);
        if (mode != 3) {
            visibility = mix(1.0, visibility, edge);
        }
        if (mode == 1) {
            visibility = min(baked_visibility, visibility);
        }
    }
    if (mode > 1) {
        visibility = min(visibility, environment_shadow_visibility());
    }
    float facing = 1.2 - abs(dot(shadow.light_direction.xyz, shadow_normal));
    float squared = facing * facing;
    return mix(visibility, mode > 1 ? 1.0 : baked_visibility,
        clamp(squared * squared, 0.0, 1.0));
}
#endif

void main() {
    vec4 sample_color = texture(surface, coordinates);
    float visibility = color.a;
#if DETAIL_PRIMARY_SHADOW
    visibility = primary_shadow_visibility(visibility);
#endif
    // Original DetailDoodad.bls pixel variant zero darkens authored shadows
    // by 30%; detail range controls opacity independently of that shadow byte.
    out_color = vec4(sample_color.rgb * color.rgb * (visibility * 0.3 + 0.7),
        sample_color.a * detail_visibility);
    // 781048 initializes the dedicated detail alpha reference to 128;
    // 7B2D30 overrides ordinary GX blend 2's reference with this value.
    if (out_color.a < (128.0 / 255.0)) { discard; }
    out_color.rgb = mix(scene.fog_color.rgb, out_color.rgb, fog_visibility);
}
