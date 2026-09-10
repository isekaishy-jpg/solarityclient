#version 450
layout(set=0, binding=0) uniform sampler2D history;
layout(set=0, binding=1) uniform sampler2D noise;
layout(push_constant) uniform Special { vec4 effect; vec4 color; } p;
layout(location=0) out vec4 outputColor;
void main() {
    ivec2 pixel = ivec2(gl_FragCoord.xy);
    // 7E92A0 draws Y=3..4 and Y=0..3 through a bottom-origin projection.
    // All other pixels retain their previous quantized values.
    if (pixel.y >= 125) outputColor = p.color;
    else if (pixel.y == 124) {
        float alpha = texture(noise, vec2(gl_FragCoord.x, p.effect.x + 0.5) / 256.0).a;
        outputColor = p.color * vec4(1.0, 1.0, 1.0, alpha);
    } else outputColor = p.effect.y != 0.0 ? vec4(0.0) : texelFetch(history, pixel, 0);
}
