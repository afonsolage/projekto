#import bevy_pbr::mesh_view_bind_group
#import bevy_pbr::mesh_struct

struct Vertex {
    [[location(0)]] position: vec3<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
};

struct WireframeMaterial {
    color: vec4<f32>;
};

[[group(1), binding(0)]]
var<uniform> material: WireframeMaterial;

[[group(2), binding(0)]]
var<uniform> mesh: Mesh;

[[stage(vertex)]]
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = view.view_proj * mesh.model * vec4<f32>(vertex.position, 1.0);

    return out;
}

[[stage(fragment)]]
fn fragment() -> [[location(0)]] vec4<f32> {
    return material.color;
}