#version 450

layout(set = 0, binding = 0) uniform sampler2D sceneColor;

layout(push_constant) uniform GlowState {
    vec4 parameters;
    vec4 sceneSampling;
    vec4 blurSampling;
} glowState;

layout(location = 0) in vec2 fragmentUv;
layout(location = 0) out vec4 outputColor;

void main() {
    const vec2 offsets[4] = vec2[](
        vec2(-1.5, -1.5), vec2(0.5, -1.5),
        vec2(0.5, 0.5), vec2(-1.5, 0.5));
    vec3 filtered = vec3(0.0);
    for (int tap = 0; tap < 4; ++tap) {
        filtered += texture(sceneColor,
            fragmentUv * glowState.sceneSampling.xy + glowState.sceneSampling.zw + offsets[tap] * glowState.parameters.xy).rgb;
    }
    outputColor = vec4(filtered * 0.25, 1.0);
}
