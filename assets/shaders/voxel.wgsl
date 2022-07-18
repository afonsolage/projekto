#import bevy_pbr::mesh_view_bind_group
#import bevy_pbr::mesh_struct

struct Vertex {
    [[location(0)]] position: vec3<f32>;
    [[location(1)]] normal: vec3<f32>;
    [[location(2)]] uv: vec2<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] light_intensity: vec3<f32>;
    [[location(1)]] uv: vec2<f32>;
};

[[group(1), binding(0)]]
var atlas_texture: texture_2d<f32>;

[[group(1), binding(1)]]
var atlas_sampler: sampler;

[[group(2), binding(0)]]
var<uniform> mesh: Mesh;

let sun_dir = vec3<f32>(0.5, 0.8, 0.3);
let ambient_intensity = vec3<f32>(0.25, 0.25, 0.25);

[[stage(vertex)]]
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = view.view_proj * mesh.model * vec4<f32>(vertex.position, 1.0);
    out.light_intensity = max(dot(vertex.normal, sun_dir), 0.0) + ambient_intensity;
    out.uv = vertex.uv;

    return out;
}

struct FragmentInput {
    [[location(0)]] light_intensity: vec3<f32>;
    [[location(1)]] uv: vec2<f32>;
};

[[stage(fragment)]]
fn fragment(in: FragmentInput) -> [[location(0)]] vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, in.uv);
    return color * vec4<f32>(in.light_intensity, 1.0);
}