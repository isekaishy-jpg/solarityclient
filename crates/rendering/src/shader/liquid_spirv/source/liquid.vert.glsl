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

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec4 in_color;
layout(location = 3) in vec2 in_depth_coordinates;
layout(location = 4) in vec2 in_surface_coordinates;

layout(location = 0) out vec2 depth_coordinates;
layout(location = 1) out vec2 surface_coordinates;
layout(location = 2) out vec4 lit_color;
layout(location = 3) out vec3 specular_color;
layout(location = 4) out float fog_visibility;

void main() {
    vec4 view_position = draw.model_view * vec4(in_position, 1.0);
    gl_Position = draw.projection * view_position;
    surface_coordinates = (draw.surface_transform * vec4(in_surface_coordinates, 0.0, 1.0)).xy;
    depth_coordinates = (draw.depth_transform * vec4(in_depth_coordinates, 0.0, 1.0)).xy;
    float fog_base = max(view_position.z * draw.fog_coefficients.x + draw.fog_coefficients.y, 0.0);
    fog_visibility = min(pow(fog_base, draw.fog_coefficients.z), 1.0);
    specular_color = vec3(0.0);

#if LIQUID_SHADER == 2
    // vsLiquidMagma passes the original vertex color without scene lighting.
    lit_color = in_color;
#else
    // vsLiquidWater transforms the normal without renormalizing it. The
    // native c33 direction follows the light ray, hence the negative dot.
    vec3 normal = mat3(draw.model_view) * in_normal;
    float direct = clamp(dot(-draw.light_direction.xyz, normal), 0.0, 1.0);
    vec3 light = draw.ambient_color.rgb + draw.diffuse_color.rgb * direct;
    // The original four vertex variants admit zero through three point lights.
    for (uint index = 0u; index < min(draw.control.x, 3u); ++index) {
        vec3 difference = draw.points[index].position.xyz - view_position.xyz;
        float distance_squared = dot(difference, difference);
        float inverse_distance = inversesqrt(distance_squared);
        float distance = 1.0 / inverse_distance;
        vec3 direction = difference * inverse_distance;
        vec3 coefficients = draw.points[index].attenuation.xyz;
        float attenuation = 1.0 / (coefficients.x + coefficients.y * distance + coefficients.z * distance * distance);
        light += draw.points[index].color.rgb * clamp(dot(direction, normal), 0.0, 1.0) * attenuation;
    }
    lit_color = vec4(light, 1.0) * in_color;
#if LIQUID_SHADER == 0
    vec3 half_direction = normalize(normalize(view_position.xyz) + draw.light_direction.xyz);
    float specular = pow(max(dot(-half_direction, normal), 0.0), draw.specular_color_power.w);
    specular_color = specular * draw.specular_color_power.rgb;
#endif
#endif
}
