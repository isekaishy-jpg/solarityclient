#version 460

#ifndef LIQUID_SHADER
#error LIQUID_SHADER must select water, water without specular, or magma
#endif

struct LiquidPointLight {
    vec4 position;
    vec4 color;
    vec4 attenuation;
};

layout(std140, set = 0, binding = 0) uniform LiquidDraw {
    mat4 projection;
    mat4 model_view;
    mat4 surface_transform;
    mat4 depth_transform;
    vec4 fog_coefficients;
    vec4 light_direction;
    vec4 ambient_color;
    vec4 diffuse_color;
    vec4 specular_color_power;
    vec4 fog_color;
    LiquidPointLight points[3];
    uvec4 control;
} draw;

layout(set = 1, binding = 0) uniform sampler2D depth_texture;
layout(set = 1, binding = 1) uniform sampler2D surface_texture;

layout(location = 0) in vec2 depth_coordinates;
layout(location = 1) in vec2 surface_coordinates;
layout(location = 2) in vec4 lit_color;
layout(location = 3) in vec3 specular_color;
layout(location = 4) in float fog_visibility;
layout(location = 0) out vec4 output_color;

void main() {
    // ps_2_0 v0/v1 color inputs saturate before shader use. Vulkan varyings
    // need this explicitly: learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-writing-shaders-9.
    vec4 diffuse_vertex = clamp(lit_color, 0.0, 1.0);
    vec3 specular_vertex = clamp(specular_color, 0.0, 1.0);
    vec4 surface = texture(surface_texture, surface_coordinates);
#if LIQUID_SHADER == 2
    vec4 result = vec4(surface.rgb * diffuse_vertex.rgb, 1.0);
#else
    vec4 depth = texture(depth_texture, depth_coordinates);
    vec4 result = vec4(diffuse_vertex.rgb * depth.rgb + surface.rgb, depth.a * diffuse_vertex.a);
#if LIQUID_SHADER == 0
    // psLiquidWater includes this constant even when the directional specular
    // color is black. The NoSpec family omits the entire alpha-weighted term.
    result.rgb += surface.a * (specular_vertex + vec3(0.25));
#endif
#endif
    // D3D's post-pixel-shader fog stage affects RGB and preserves liquid alpha.
    result.rgb = mix(draw.fog_color.rgb, result.rgb, clamp(fog_visibility, 0.0, 1.0));
    output_color = result;
}
