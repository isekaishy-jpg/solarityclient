#version 450

layout(set = 0, binding = 0) uniform sampler2D sceneColor;
layout(set = 0, binding = 1) uniform sampler2D blurredScene;

layout(push_constant) uniform GlowState {
    vec4 parameters;
    vec4 sceneSampling;
    vec4 blurSampling;
} glowState;

layout(location = 0) in vec2 fragmentUv;
layout(location = 0) out vec4 outputColor;

void main() {
    vec4 scene = texture(sceneColor, fragmentUv * glowState.sceneSampling.xy + glowState.sceneSampling.zw);
    vec3 blurred = texture(blurredScene, fragmentUv * glowState.blurSampling.xy + glowState.blurSampling.zw).rgb;
    vec3 composed = scene.rgb + blurred * blurred * glowState.parameters.x;
    composed = pow(max(composed, vec3(0.0)),
                   vec3(1.0 / max(glowState.parameters.y, 0.01)));
    outputColor = vec4(composed, scene.a);
}
