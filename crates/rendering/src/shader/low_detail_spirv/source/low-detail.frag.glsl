#version 450
// 7D5E70 forces fog start/end 0/1 and the DayNight +8C packed fog color.
layout(push_constant) uniform Horizon {
    mat4 view;
    vec4 projection;
    vec4 fogColor;
} scene;
layout(location = 0) out vec4 color;
void main() { color = vec4(scene.fogColor.rgb, 1.0); }
