#version 460

layout(constant_id = 1) const bool ALPHA_TEST = false;
#ifdef SHADOW_WMO
layout(set = 2, binding = 0) uniform sampler2D model_texture;
const float ALPHA_REFERENCE = 224.0 / 255.0;
#else
layout(set = 3, binding = 0) uniform sampler2D model_texture;
const float ALPHA_REFERENCE = 128.0 / 255.0;
#endif
layout(location = 0) in vec2 fragment_coordinates;
layout(location = 1) in float fragment_depth;
layout(location = 0) out float out_depth;

void main() {
    // 82DA40 sets 128/255 for textured casters; ShadowMapSL keeps equality.
    if (ALPHA_TEST && texture(model_texture, fragment_coordinates).a < ALPHA_REFERENCE) {
        discard;
    }
    out_depth = max(fragment_depth * 0.00025, 0.0);
}
