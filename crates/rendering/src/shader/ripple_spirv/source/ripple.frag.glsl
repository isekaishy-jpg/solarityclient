#version 460

layout(push_constant) uniform RippleFrame {
    mat4 view_projection;
    float depth_bias;
} frame;
layout(set = 0, binding = 0) uniform sampler2D surface;
layout(location = 0) in vec4 color;
layout(location = 1) in vec2 coordinates;
layout(location = 0) out vec4 out_color;

void main() {
    out_color = texture(surface, coordinates) * color;
    // GX blend 3 selects alpha reference 1 (AD8B7C); the D3D9/Ex
    // initialization uses GREATER_OR_EQUAL (6A3AA0 / 6A7A40).
    if (out_color.a < (1.0 / 255.0)) {
        discard;
    }
    // D3D9 applies its bias directly in normalized depth units. Vulkan's
    // ordinary raster bias scales with the depth format and primitive.
    gl_FragDepth = clamp(gl_FragCoord.z + frame.depth_bias, 0.0, 1.0);
}
