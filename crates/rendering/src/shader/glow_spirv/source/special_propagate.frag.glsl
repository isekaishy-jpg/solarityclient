#version 450
layout(set=0, binding=0) uniform sampler2D history;
layout(push_constant) uniform Special { vec4 effect; } p;
layout(location=0) in vec2 fragmentUv;
layout(location=0) out vec4 outputColor;
void main() {
    // FFXPropagateFog's original tap order and native constructor offsets.
    precise vec4 result = texture(history, fragmentUv + vec2(0.0, 1.0) / vec2(256.0, 128.0)) * 0.25;
    result += texture(history, fragmentUv + vec2(-1.0, 1.0) / vec2(256.0, 128.0)) * 0.25;
    result += texture(history, fragmentUv + vec2(1.0, 1.0) / vec2(256.0, 128.0)) * 0.25;
    result += texture(history, fragmentUv + vec2(0.0, 2.0) / vec2(256.0, 128.0)) * 0.25;
    result.a -= p.effect.x;
    outputColor = result;
}
