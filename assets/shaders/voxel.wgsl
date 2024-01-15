#import bevy_pbr::mesh_functions::{get_model_matrix, mesh_position_local_to_clip}

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tile_coord_start: vec2<f32>,
    @location(4) light: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) light_intensity: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tile_coord_start: vec2<f32>,
};

struct MaterialData {
    tile_texture_size: f32,
};

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;

@group(1) @binding(1)
var atlas_sampler: sampler;

@group(1) @binding(2)
var<uniform> material_data: MaterialData;

@vertex
fn vertex(
    vertex: Vertex,
) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = mesh_position_local_to_clip(get_model_matrix(vertex.instance_index), vec4<f32>(vertex.position, 1.0));
    out.light_intensity = vertex.light;
    out.uv = vertex.uv;
    out.tile_coord_start = vertex.tile_coord_start;

    return out;
}

struct FragmentInput {
    @location(0) light_intensity: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tile_coord_start: vec2<f32>,
};

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let tiled_coord = in.uv % material_data.tile_texture_size;
    var color = textureSample(atlas_texture, atlas_sampler, in.tile_coord_start + tiled_coord);

    return color * vec4<f32>(in.light_intensity, 1.0);
}
