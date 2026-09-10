#version 450
layout(push_constant) uniform Special { vec4 effect; vec4 extent; } p;
layout(location=0) out vec2 historyUv;
layout(location=1) out vec2 sceneUv;
void main() {
    const int corners[6] = int[6](1, 33, 0, 1, 34, 33);
    int cell = gl_VertexIndex / 6;
    int index = (cell / 32) * 33 + cell % 32 + corners[gl_VertexIndex % 6];
    vec2 grid = vec2(index % 33, index / 33) * 0.03125;
    vec2 uv = vec2(grid.x, 1.0 - grid.y);
    gl_Position = vec4(uv * 2.0 - 1.0 + 1.0 / p.extent.xy, 0.0, 1.0);
    vec2 axis = (uv - 0.5) * 1.414213657;
    float radius = length(axis);
    float angle = radius == 0.0 ? 0.0 : acos(clamp(axis.y / radius, -1.0, 1.0));
    historyUv = vec2(angle * 0.3183098733, radius) + 0.5 / vec2(256.0, 128.0);
    sceneUv = uv + 0.5 / p.extent.xy;
}
