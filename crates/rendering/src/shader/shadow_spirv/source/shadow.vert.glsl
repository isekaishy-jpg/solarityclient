#version 460

layout(constant_id = 0) const int M2_BONE_CLASS = 0;

layout(set = 0, binding = 0) uniform ShadowScene {
    mat4 projection;
    mat4 view;
    vec4 origin;
} scene;
layout(std430, set = 1, binding = 0) readonly buffer M2BoneTransforms {
    mat4 transforms[];
} bones;
struct M2Instance {
    mat4 model;
    mat4 texture_transforms[2];
    mat4 model_view;
    vec4 mesh_color;
    vec4 fog_color;
    vec4 fragment_parameters;
    vec4 liquid_clip_plane;
    uvec4 palette;
};
layout(std430, set = 2, binding = 0) readonly buffer M2Instances {
    M2Instance records[];
} instances;
#define material instances.records[gl_InstanceIndex]


layout(location = 0) in vec3 model_position;
layout(location = 1) in vec4 bone_weights;
layout(location = 2) in uvec4 bone_indices;
layout(location = 4) in vec2 texture_coordinates;
layout(location = 0) out vec2 fragment_coordinates;
layout(location = 1) out float fragment_depth;

// ShadowMap.bls shares the ordinary SKIN rigid/one/four-influence classes.
mat4 skin_matrix() {
    if (M2_BONE_CLASS == 0 || material.palette.y == 0u) {
        return mat4(1.0);
    }
    if (M2_BONE_CLASS == 1) {
        return bones.transforms[material.palette.x + bone_indices.x];
    }
    mat4 skin = mat4(0.0);
    skin += bones.transforms[material.palette.x + bone_indices.x] * bone_weights.x;
    skin += bones.transforms[material.palette.x + bone_indices.y] * bone_weights.y;
    skin += bones.transforms[material.palette.x + bone_indices.z] * bone_weights.z;
    skin += bones.transforms[material.palette.x + bone_indices.w] * bone_weights.w;
    return skin;
}

void main() {
    vec4 world_position = material.model * skin_matrix() * vec4(model_position, 1.0);
    precise vec4 eye = scene.view * vec4(world_position.xyz - scene.origin.xyz, 1.0);
    gl_Position = scene.projection * eye;
    fragment_depth = eye.z;
    // 7BBC50 calls 873480(0), resetting the first UV transform to identity.
    fragment_coordinates = texture_coordinates;
}
