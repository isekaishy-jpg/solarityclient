#version 450
layout(set=0,binding=0) uniform sampler2D sceneImage;
layout(set=0,binding=1) uniform sampler2D blurImage;
layout(push_constant) uniform Parameters {
    vec4 effect;
    vec4 scene;
    vec4 blur;
} p;
layout(location=0) in vec2 uv;
layout(location=0) out vec4 color;
void main() {
    vec3 original = texture(sceneImage, uv * p.scene.xy + p.scene.zw).rgb;
    vec3 combined = texture(blurImage, uv * p.blur.xy + p.blur.zw).rgb + original * 0.5;
    float gray = dot(combined, vec3(1.0)) * 0.166666493;
    combined = (combined * 0.5 + gray) * vec3(0.6, 0.6, 0.78);
    color = vec4(original + p.effect.x * (combined - original), 1.0);
}
