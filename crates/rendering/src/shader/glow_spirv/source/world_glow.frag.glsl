#version 450

layout(set = 0, binding = 0) uniform sampler2D sceneColor;
layout(set = 0, binding = 1) uniform sampler2D blurredScene;
layout(set = 0, binding = 2) uniform sampler2D waveTexture;

layout(push_constant) uniform GlowState {
    vec4 parameters;
    vec4 sceneSampling;
    vec4 blurSampling;
    vec4 waveX;
    vec4 waveY;
} glowState;

layout(location = 0) in vec2 fragmentUv;
layout(location = 0) out vec4 outputColor;

void main() {
    vec2 sceneUv = fragmentUv * glowState.sceneSampling.xy + glowState.sceneSampling.zw;
    vec2 blurUv = fragmentUv * glowState.blurSampling.xy + glowState.blurSampling.zw;
    if (glowState.parameters.z != 0.0) {
        vec3 coordinate = vec3(fragmentUv, 1.0);
        vec2 waveUv = vec2(dot(glowState.waveX.xyz, coordinate),
                           dot(glowState.waveY.xyz, coordinate));
        vec2 displacement = texture(waveTexture, waveUv).rg;
        sceneUv += displacement * (3.0 / vec2(textureSize(sceneColor, 0)));
        blurUv += displacement * (0.75 / vec2(textureSize(blurredScene, 0)));
    }
    vec3 scene = texture(sceneColor, sceneUv).rgb;
    vec3 blurred = texture(blurredScene, blurUv).rgb;
    vec3 composed = mix(scene, blurred, glowState.parameters.y)
                  + blurred * blurred * glowState.parameters.x;
    outputColor = vec4(composed, 1.0);
}
