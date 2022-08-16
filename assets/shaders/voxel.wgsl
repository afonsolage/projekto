#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_types

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tile_coord_start: vec2<f32>,
    @location(4) light: vec3<f32>,
    // @location(5) occlusion: vec4<u32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) light_intensity: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tile_coord_start: vec2<f32>,
};

struct MaterialData {
    tile_texture_size: f32,
    clip_map_origin: vec2<f32>,
    clip_map: array<vec4<f32>,256>,
    // clip_height: f32,
};

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;

@group(1) @binding(1)
var atlas_sampler: sampler;

@group(1) @binding(2)
var<uniform> material_data: MaterialData;

@group(2) @binding(0)
var<uniform> mesh: Mesh;

let clipped_vertex: vec4<f32> = vec4<f32>(-2.0, -2.0, -2.0, -2.0);
let clipped_light: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);
let clipped_tile_coord_start: vec2<f32> = vec2<f32>(0.0, 0.0);

// let right_side_mask: u32 = 1u;
// let left_side_mask: u32 = 2u;
// let up_side_mask: u32 = 4u;
// let down_side_mask: u32 = 8u;
// let front_side_mask: u32 = 16u;
// let back_side_mask: u32 = 32u;

let no_clip_height: f32 = 9999.0;
let chunk_axis_size: vec2<u32> = vec2<u32>(16u, 16u);

//(116, 116) -> (115, 115)
//(150, 150) -> (150, 150) - (100, 100) -> |(50, 50)| > (16, 16) -> false
//(95, 95) -> (95, 95) - (100, 100) -> |(-5, -5)| > (16, 16) -> true
//(110, 110) -> (110, 110) - (100, 100) -> |(10, 10)| > (16, 16) -> true

fn is_on_clip_map_bounds(position: vec2<f32>) -> bool {
    if (position.x >= 0.0 && position.x < f32(chunk_axis_size.x) && position.y >= 0.0 && position.y < f32(chunk_axis_size.y)) {
        return true;
    } else {
        return false;
    }
}

fn calc_clip_height(vertex: Vertex) -> f32 {
    var vertex_world_pos = vec4<f32>(vertex.position, 1.0) * mesh.model;
    var vertex_clip_map_pos = vertex_world_pos.xz - material_data.clip_map_origin;

    if (is_on_clip_map_bounds(vertex_clip_map_pos) == false) {
        return no_clip_height;
    }

    var clip_map_index = u32(vertex_clip_map_pos.x) * chunk_axis_size.y + u32(vertex_clip_map_pos.y);
    return material_data.clip_map[clip_map_index].x;
}

@vertex
fn vertex(
    vertex: Vertex,
    @builtin(vertex_index) vertex_index: u32
) -> VertexOutput {
    var out: VertexOutput;

    var position = vec4<f32>(vertex.position, 1.0);
    var light_intensity = vertex.light;
    var tile_coord_start = vertex.tile_coord_start;

    var clip_height = calc_clip_height(vertex);
    // var face_vertex_idx = vertex_index % u32(4);
    // var occlusion = vertex.occlusion[face_vertex_idx];

    // Top Face
    if (vertex.normal.y > 0.0) {
        if (vertex.position.y == clip_height) {
            light_intensity = clipped_light;
            tile_coord_start = clipped_tile_coord_start;
        } else if (vertex.position.y > clip_height) {
            position = clipped_vertex;
        }
    } else if (vertex.normal.y < 0.0 && vertex.position.y >= clip_height) {
        // Always clip bottom vertices.
        // TODO: Don't sent bottom faces to shader.
        position = clipped_vertex;
    } else if (vertex.normal.y == 0.0 && vertex.position.y > clip_height) {
        // Clip non-top faces
        position.y = clip_height;
    }

    out.clip_position = view.view_proj * mesh.model * position;
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