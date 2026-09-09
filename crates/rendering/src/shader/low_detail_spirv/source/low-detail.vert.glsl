#version 450
// 791170/795F80: staged fixed-function view and horizon projection.
layout(location = 0) in vec3 position;
layout(push_constant) uniform Horizon {
    mat4 view;
    vec4 projection;
    vec4 fogColor;
} scene;
void main() {
    precise vec4 eye = scene.view * vec4(position, 1.0);
    gl_Position = vec4(eye.x * scene.projection.x, eye.y * scene.projection.y,
                      eye.z * scene.projection.z + scene.projection.w, -eye.z);
}
