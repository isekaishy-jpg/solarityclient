#version 450

layout(set = 0, binding = 0) uniform sampler2D sceneColor;

layout(push_constant) uniform GlowState {
    vec4 parameters;
} glowState;

layout(location = 0) in vec2 fragmentUv;
layout(location = 0) out vec4 outputColor;

void main() {
    const vec4 offsets = vec4(-2.5, -0.5, 0.5, 2.5);
    const vec4 weights = vec4(0.125, 0.375, 0.375, 0.125);
    vec2 direction = glowState.parameters.xy * glowState.parameters.zw;
    vec3 blurred = vec3(0.0);
    for (int tap = 0; tap < 4; ++tap) {
        blurred += texture(sceneColor, fragmentUv + direction * offsets[tap]).rgb
            * weights[tap];
    }
    outputColor = vec4(blurred, 1.0);
}
