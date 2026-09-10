#version 450
layout(set=0, binding=0) uniform sampler2D history;
layout(set=0, binding=1) uniform sampler2D scene;
layout(push_constant) uniform Special { vec4 effect; } p;
layout(location=0) in vec2 historyUv;
layout(location=1) in vec2 sceneUv;
layout(location=0) out vec4 outputColor;
void main() {
    vec3 color = texture(scene, sceneUv).rgb;
    vec4 fog = texture(history, historyUv);
    float gray = dot(color, vec3(0.333332986));
    color = mix(color, vec3(gray), clamp(p.effect.x, 0.0, 1.0));
    color = mix(color, vec3(1.0), clamp(p.effect.y, 0.0, 1.0));
    outputColor = vec4(mix(color, fog.rgb, fog.a), 1.0);
}
