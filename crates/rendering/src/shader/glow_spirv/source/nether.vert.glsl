#version 450
// 8C0740/8C0DE0's 6x6 mesh and archive FFXNetherBlur vertex program.
layout(push_constant) uniform Nether {
    vec4 effect;
    vec4 extent;
    uvec4 colors[3];
} p;
layout(location=0) out vec2 uv0;
layout(location=1) out vec2 uv1;
layout(location=2) out vec2 uv2;
layout(location=3) out vec2 uv3;
void main() {
    const int corners[6] = int[6](0, 7, 6, 0, 1, 7);
    int cell = gl_VertexIndex / 6;
    int index = (cell / 5) * 6 + cell % 5 + corners[gl_VertexIndex % 6];
    ivec2 grid = ivec2(index % 6, index / 6);
    vec2 pixel = vec2(0.0);
    vec2 stepPixel = p.extent.zw * 0.2;
    for (int i=0; i<grid.x; ++i) pixel.x += stepPixel.x;
    for (int i=0; i<grid.y; ++i) pixel.y += stepPixel.y;
    vec2 clip = pixel * (2.0 / p.extent.zw) - 1.0;
    // D3D's integer pixel centers become Vulkan's half-pixel centers.
    gl_Position = vec4(clip + 1.0 / p.extent.zw, 0.0, 1.0);
    uint word = p.colors[(index / 4) / 4][(index / 4) % 4];
    float noise = float((word >> uint((index % 4) * 8)) & 255u) / 255.0;
    float angle = ((noise * 2.0 - 1.0) * 3.0 + p.effect.x) * 0.159154937;
    angle = mod(fract(abs(angle)) * sign(angle) + 0.5, 1.0) * 6.28318548 - 3.14159274;
    vec2 direction = vec2(cos(angle), sin(angle));
    vec2 uv = vec2(0.5 / p.extent.x, 1.0 + 0.5 / p.extent.y);
    for (int i=0; i<grid.x; ++i) uv.x += 0.2;
    for (int i=0; i<grid.y; ++i) uv.y -= 0.2;
    // These fixed archive constants are independent of the target extent.
    uv += vec2(-0.001953125, -0.00260416674);
    uv0 = uv;
    uv1 = uv + direction * p.effect.y;
    uv2 = uv1 + direction * p.effect.y;
    uv3 = uv2 + direction * p.effect.y;
}
