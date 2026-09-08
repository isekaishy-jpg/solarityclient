#version 460

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec2 in_texture_coordinates;
layout(location = 3) in vec2 in_alpha_coordinates;
layout(location = 4) in vec4 in_color_bgra;

layout(set = 0, binding = 0) uniform TerrainScene {
    mat4 view_projection;
    vec4 ambient_color;
    vec4 diffuse_color;
    vec4 sun_direction;
    vec4 view_depth;
    vec4 fog_parameters;
    vec4 fog_color;
} scene;

layout(push_constant) uniform TerrainDraw {
    uvec2 atlas_chunk;
} draw;

layout(location = 0) out vec3 out_normal;
layout(location = 1) out vec2 out_texture_coordinates;
layout(location = 2) out vec2 out_atlas_coordinates;
layout(location = 3) out vec3 out_vertex_light;
layout(location = 4) out float out_fog_visibility;

void main() {
    const float chunk_texels = 64.0;
    const float atlas_texels = 1024.0;
    vec2 atlas_origin = vec2(draw.atlas_chunk) * chunk_texels;

    gl_Position = scene.view_projection * vec4(in_position, 1.0);
    // Original Terrain.bls uses view Z and c12, then pow/max/min into oFog.
    float eye_depth = -dot(scene.view_depth, vec4(in_position, 1.0));
    float inverse_range = 1.0 / (scene.fog_parameters.y - scene.fog_parameters.x);
    float visibility = max((scene.fog_parameters.y - eye_depth) * inverse_range, 0.0);
    out_fog_visibility = scene.fog_color.w > 0.0
        ? min(pow(visibility, scene.fog_parameters.w), 1.0) : 1.0;
    out_normal = in_normal;
    out_texture_coordinates = in_texture_coordinates;
    out_atlas_coordinates = (atlas_origin + in_alpha_coordinates * chunk_texels) / atlas_texels;

    // MCCV is stored BGRA, and stock's neutral 0x7f value represents unit
    // modulation rather than half intensity.
    out_vertex_light = min(in_color_bgra.bgr * (255.0 / 127.0), vec3(1.0));
}
