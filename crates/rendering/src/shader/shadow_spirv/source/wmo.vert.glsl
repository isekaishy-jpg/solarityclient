#version 460

layout(set = 0, binding = 0) uniform ShadowScene {
    mat4 projection;
    mat4 view;
    vec4 origin;
} scene;
layout(std140, set = 1, binding = 0) uniform WorldModelMaterialState {
    mat4 model;
} material;
layout(location = 0) in vec3 model_position;
layout(location = 4) in vec2 texture_coordinates;
layout(location = 0) out vec2 fragment_coordinates;
layout(location = 1) out float fragment_depth;

void main() {
    vec4 world_position = material.model * vec4(model_position, 1.0);
    precise vec4 eye = scene.view * vec4(world_position.xyz - scene.origin.xyz, 1.0);
    gl_Position = scene.projection * eye;
    fragment_depth = eye.z;
    // 7AB760 supplies the group's first untransformed texture coordinates.
    fragment_coordinates = texture_coordinates;
}
