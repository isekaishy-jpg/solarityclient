#version 450
layout(set=0,binding=0) uniform sampler2D sourceImage;
layout(location=0) in vec2 uv0;
layout(location=1) in vec2 uv1;
layout(location=2) in vec2 uv2;
layout(location=3) in vec2 uv3;
layout(location=0) out vec4 color;
void main() {
    color = texture(sourceImage, uv1) * 0.25;
    color += texture(sourceImage, uv0) * 0.25;
    color += texture(sourceImage, uv2) * 0.25;
    color += texture(sourceImage, uv3) * 0.25;
}
