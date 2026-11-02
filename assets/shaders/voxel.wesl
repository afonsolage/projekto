import bevy_pbr::render::mesh_functions::{
    get_world_from_local,
    mesh_position_local_to_clip,
    mesh_position_local_to_world,
};

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tile_coord_start: vec2<f32>,
    @location(4) light: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) light_intensity: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tile_coord_start: vec2<f32>,
    @location(3) world_position: vec3<f32>,
}

const CHUNK_SIZE: f32 = 16.0;

struct MaterialData {
    tile_texture_size: f32,
    debug_chunk_grid: f32,
    _padding: vec2<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var atlas_texture: texture_2d<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(1)
var atlas_sampler: sampler;

@group(constants::MATERIAL_BIND_GROUP) @binding(2)
var<uniform> material_data: MaterialData;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = get_world_from_local(vertex.instance_index);
    let world_position =
        mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));

    out.clip_position = mesh_position_local_to_clip(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.light_intensity = vertex.light;
    out.uv = vertex.uv;
    out.tile_coord_start = vertex.tile_coord_start;
    out.world_position = world_position.xyz;

    return out;
}

struct FragmentInput {
    @location(0) light_intensity: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tile_coord_start: vec2<f32>,
    @location(3) world_position: vec3<f32>,
}

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let tiled_coord = in.uv % material_data.tile_texture_size;
    var color = textureSample(atlas_texture, atlas_sampler, in.tile_coord_start + tiled_coord);

    if (material_data.debug_chunk_grid > 0.5) {
        let cell = floor(in.world_position.xz / CHUNK_SIZE);
        let local = in.world_position.xz - cell * CHUNK_SIZE;
        let edge = min(local, vec2<f32>(CHUNK_SIZE) - local);

        // A face lying exactly on a chunk boundary has a constant coordinate in
        // that axis. Suppress that axis instead of flooding the whole wall with
        // the grid colour.
        let x_derivative = fwidth(in.world_position.x);
        let z_derivative = fwidth(in.world_position.z);
        let x_width = max(x_derivative * 1.5, 0.0001);
        let z_width = max(z_derivative * 1.5, 0.0001);
        let x_line =
            (1.0 - smoothstep(0.0, x_width, edge.x)) * step(0.0001, x_derivative);
        let z_line =
            (1.0 - smoothstep(0.0, z_width, edge.y)) * step(0.0001, z_derivative);
        let line = max(x_line, z_line);

        if (line > 0.01) {
            let hash = fract(sin(dot(cell, vec2<f32>(12.9898, 78.233))) * 43758.5453);
            let debug_color = vec3<f32>(
                0.55 + 0.45 * fract(hash * 3.17),
                0.55 + 0.45 * fract(hash * 7.31),
                0.55 + 0.45 * fract(hash * 11.73),
            );
            color = vec4<f32>(mix(color.rgb, debug_color, line * 0.85), color.a);
        }
    }

    return color * vec4<f32>(in.light_intensity, 1.0);
}
