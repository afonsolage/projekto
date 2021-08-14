#version 450
layout(location = 0) in vec3 Vertex_Position;
layout(location = 1) in vec3 Vertex_Normal;

layout(location = 0) out vec3 o_Light_Intensity;

layout(set = 0, binding = 0) uniform CameraViewProj {
    mat4 ViewProj;
};
layout(set = 1, binding = 0) uniform Transform {
    mat4 Model;
};

vec3 sun_dir = vec3(0.5, 0.8, 0.3);
vec3 ambiente_intensity = vec3(0.25, 0.25, 0.25);

void main() {
    gl_Position = ViewProj * Model * vec4(Vertex_Position, 1.0);
    o_Light_Intensity = max(dot(Vertex_Normal.xyz, sun_dir.xyz), 0.0) + ambiente_intensity;
}