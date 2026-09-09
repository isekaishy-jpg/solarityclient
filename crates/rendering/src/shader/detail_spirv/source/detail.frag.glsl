#version 460

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

void main() {
    vec4 sample_color = texture(surface, coordinates);
    // Original DetailDoodad.bls pixel variant zero darkens authored shadows
    // by 30%; detail range controls opacity independently of that shadow byte.
    out_color = vec4(sample_color.rgb * color.rgb * (color.a * 0.3 + 0.7),
        sample_color.a * detail_visibility);
    // 781048 initializes the dedicated detail alpha reference to 128;
    // 7B2D30 overrides ordinary GX blend 2's reference with this value.
    if (out_color.a < (128.0 / 255.0)) { discard; }
    out_color.rgb = mix(scene.fog_color.rgb, out_color.rgb, fog_visibility);
}
