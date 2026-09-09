#version 460

#ifndef DETAIL_PRIMARY_SHADOW
#define DETAIL_PRIMARY_SHADOW 0
#endif

#if DETAIL_PRIMARY_SHADOW
layout(std140, set = 2, binding = 0) uniform DetailShadow {
    vec4 origin_and_texel;
    vec4 receiver_rows[3];
    vec4 light_direction;
} shadow;
layout(set = 2, binding = 1) uniform sampler2D primary_shadow_map;
layout(location = 4) in vec3 shadow_coordinates;
layout(location = 5) in vec3 shadow_normal;
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
// DetailDoodad.bls pixel variant one uses five taps, the baked visibility
// minimum, and normal relief. 875D30's primary fade plane is identically zero.
float primary_shadow_visibility(float baked_visibility) {
    float edge = clamp(max(abs(shadow_coordinates.x), abs(shadow_coordinates.y))
        * -3.4482758 + 3.41379309, 0.0, 1.0);
    if (edge <= 0.0) {
        return baked_visibility;
    }
    vec2 coordinates = shadow_coordinates.xy * 0.5 + vec2(0.5);
    const vec2 offsets[5] = vec2[5](vec2(0.0), vec2(0.8, -1.0), vec2(0.2, -0.6),
        vec2(-0.6, -0.2), vec2(-1.0, -0.4));
    float visibility = 0.0;
    for (int index = 0; index < 5; ++index) {
        float depth = textureLod(primary_shadow_map,
            coordinates + offsets[index] * shadow.origin_and_texel.w, 0.0).r;
        visibility += float(depth >= shadow_coordinates.z);
    }
    visibility = min(baked_visibility, 1.0 + edge * (visibility * 0.2 - 1.0));
    float facing = 1.2 - abs(dot(shadow.light_direction.xyz, shadow_normal));
    float squared = facing * facing;
    return mix(visibility, baked_visibility, clamp(squared * squared, 0.0, 1.0));
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
