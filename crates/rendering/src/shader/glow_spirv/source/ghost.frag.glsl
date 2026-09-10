#version 450

layout(set = 0, binding = 0) uniform sampler2D sceneColor;
layout(set = 0, binding = 1) uniform sampler2D blurredScene;

layout(push_constant) uniform EffectState {
    vec4 parameters;
    vec4 sceneSampling;
    vec4 blurSampling;
} effectState;

layout(location = 0) in vec2 fragmentUv;
layout(location = 0) out vec4 outputColor;

void main() {
    vec3 scene = texture(sceneColor,
        fragmentUv * effectState.sceneSampling.xy + effectState.sceneSampling.zw).rgb;
    vec3 blurred = texture(blurredScene,
        fragmentUv * effectState.blurSampling.xy + effectState.blurSampling.zw).rgb;
    vec3 composed = blurred * blurred * effectState.parameters.x + scene;
    // The native FFXDeath shader uses 0.144 for blue, not the usual 0.114.
    float gray = clamp(dot(composed, vec3(0.299, 0.587, 0.144)), 0.0, 1.0);
    float tint = clamp(gray * (1.0 - gray) * 4.0, 0.0, 1.0);
    outputColor = vec4(vec3(gray) + vec3(83.0, 147.0, 168.0) / 255.0 * tint, 1.0);
}
