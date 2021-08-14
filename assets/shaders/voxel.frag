#version 450
layout(location = 0) out vec4 o_Target;
layout(location = 0) in vec3 o_Light_Intensity;

void main() {
    o_Target = vec4(0.7, 0.3, 0.3, 1.0) * vec4(o_Light_Intensity, 1.0);
}