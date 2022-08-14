#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_types

struct Vertex {
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
    clip_height: f32,
};

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;

@group(1) @binding(1)
var atlas_sampler: sampler;

@group(1) @binding(2)
var<uniform> material_data: MaterialData;

@group(2) @binding(0)
var<uniform> mesh: Mesh;

let clipped_vertex: vec3<f32> = vec3<f32>(-2.0, -2.0, -2.0);
let clipped_light: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);
let clipped_tile_coord_start: vec2<f32> = vec2<f32>(0.0, 0.0);

@vertex
fn vertex(
    vertex: Vertex, 
    @builtin(vertex_index) vertex_index: u32
) -> VertexOutput {
    var out: VertexOutput;

    var clip_height = material_data.clip_height;

    var position = vertex.position;
    var light_intensity = vertex.light;
    var tile_coord_start = vertex.tile_coord_start;

    if vertex.normal.y > 0.0 && vertex.position.y >= clip_height {
        position.y = clip_height;
        light_intensity = clipped_light;
        tile_coord_start = clipped_tile_coord_start;
    } else if vertex.normal.y < 0.0 && vertex.position.y >= clip_height {
        position = clipped_vertex;
    } else {
        var vertex_index_mod = vertex_index % u32(4);

        if vertex.normal.y == 0.0 && 
            (vertex.position.y > clip_height ||
                vertex.position.y == clip_height && vertex_index_mod < u32(2)) {
            position = clipped_vertex;
        }
    }

    out.clip_position = view.view_proj * mesh.model * vec4<f32>(position, 1.0);
    out.light_intensity = light_intensity;
    out.uv = vertex.uv;
    out.tile_coord_start = tile_coord_start;

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
    let color = textureSample(atlas_texture, atlas_sampler, in.tile_coord_start + tiled_coord);

    return color * vec4<f32>(in.light_intensity, 1.0);
}