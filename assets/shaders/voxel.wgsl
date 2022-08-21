#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_types

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tile_coord_start: vec2<f32>,
    @location(4) light: vec3<f32>,
    @location(5) voxel: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) light_intensity: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tile_coord_start: vec2<f32>,
    @location(3) world_normal: vec3<f32>,
    @location(4) world_pos: vec3<f32>,
};

struct MaterialData {
    tile_texture_size: f32,
    clip_map_origin: vec2<f32>,
    clip_height: f32,
};

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;

@group(1) @binding(1)
var atlas_sampler: sampler;

@group(1) @binding(2)
var<uniform> material_data: MaterialData;

@group(1) @binding(3)
var clip_map: texture_1d<u32>;

@group(2) @binding(0)
var<uniform> mesh: Mesh;

let CLIPPED_VERTEX: vec4<f32> = vec4<f32>(-2.0, -2.0, -2.0, -2.0);
let CLIPPED_LIGHT: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);
let CLIPPED_TILE_COORD_START: vec2<f32> = vec2<f32>(0.0, 0.0);

let NO_CLIP: f32 = 9999.0;
let CLIP_AXIS_SIZE: u32 = 144u;
let CLIP_SIZE: u32 = 20736u;

let UP: vec3<f32> = vec3<f32>(0.0, 1.0, 0.0);
let DOWN: vec3<f32> = vec3<f32>(0.0, -1.0, 0.0);
let RIGHT: vec3<f32> = vec3<f32>(1.0, 0.0, 0.0);
let LEFT: vec3<f32> = vec3<f32>(-1.0, 0.0, 0.0);
let FRONT: vec3<f32> = vec3<f32>(0.0, 0.0, 1.0);
let BACK: vec3<f32> = vec3<f32>(0.0, 0.0, -1.0);

fn to_world(position: vec3<f32>) -> vec4<f32> {
    return mesh.model * vec4<f32>(position, 1.0);
}

fn unpack_voxel(packed: u32) -> vec3<f32> {
    return unpack4x8unorm(packed).xyz * 255.0;
}

fn get_voxel_index(voxel: vec3<f32>) -> u32 {
    let world = to_world(voxel);
    let landscape = world.xz - material_data.clip_map_origin;
    return u32(landscape.x) * CLIP_AXIS_SIZE + u32(landscape.y);
}

fn get_voxel_clip_data(voxel: vec3<f32>) -> u32 {
    let index = i32(get_voxel_index(voxel));
    if (index < 0 || index >= i32(CLIP_SIZE)) {
        return 0u;
    } else {
        return textureLoad(clip_map, index, 0).x;
    }
}

fn is_top_face(vertex: Vertex) -> bool {
    return vertex.normal.y > 0.0;
}

fn is_side_face(vertex: Vertex) -> bool {
    return vertex.normal.y == 0.0;
}

fn get_side_clip(voxel: vec3<f32>) -> u32 {
    let clip = get_voxel_clip_data(voxel + RIGHT);
    if (clip > 0u) {
        return clip;
    }
    let clip = get_voxel_clip_data(voxel + LEFT);
    if (clip > 0u) {
        return clip;
    }
    let clip = get_voxel_clip_data(voxel + FRONT);
    if (clip > 0u) {
        return clip;
    }
    let clip = get_voxel_clip_data(voxel + BACK);
    if (clip > 0u) {
        return clip;
    } else {
        return 0u;
    }
}

@vertex
fn vertex(
    vertex: Vertex,
) -> VertexOutput {
    var out: VertexOutput;

    var position = vec4<f32>(vertex.position, 1.0);
    var light_intensity = vertex.light;
    var tile_coord_start = vertex.tile_coord_start;
    var should_clip = false;

    if (material_data.clip_height < NO_CLIP) {
        let voxel = unpack_voxel(vertex.voxel);
        let neighbor = voxel + vertex.normal;

        let neighbor_clip_height = f32(get_voxel_clip_data(neighbor));
        let voxel_clip_height = f32(get_voxel_clip_data(voxel));

        if (is_top_face(vertex)) {
            // If the top voxel is equals or bellow the clipping, do nothing
            if (voxel.y <= voxel_clip_height) {
                //Do nothing

            // If current voxel isn't on line on sight but is on the side of a voxel, which is on line of sight.
            } else if (voxel_clip_height == 0.0 && get_side_clip(voxel) > 0u) {
                light_intensity = CLIPPED_LIGHT;
                tile_coord_start = CLIPPED_TILE_COORD_START;
                position.y = material_data.clip_height + 1.0;

            // If there is some visible voxel bellow it
            } else if (voxel_clip_height > 0.0 && voxel_clip_height < material_data.clip_height) {
                light_intensity = CLIPPED_LIGHT;
                tile_coord_start = CLIPPED_TILE_COORD_START;
                position.y = material_data.clip_height + 1.0;

            // If current voxel isn't on light on sight and isn't on the side of a voxel which is
            } else if (voxel_clip_height > 0.0) {
                should_clip = true;

            // Just clip everything else
            } else {
                light_intensity = CLIPPED_LIGHT;
                tile_coord_start = CLIPPED_TILE_COORD_START;
                position.y = material_data.clip_height + 1.0;
            }
        } else if (is_side_face(vertex)) {
            // Only clip side faces that isn't on line of sight
            if (voxel.y > voxel_clip_height && voxel.y > neighbor_clip_height) {
                should_clip = true;
            }
        } else {
            should_clip = true;
        }
    }

    if (should_clip) {
        out.clip_position = CLIPPED_VERTEX;
    } else {
        out.clip_position = view.view_proj * mesh.model * position ;
    }

    out.light_intensity = light_intensity;
    out.uv = vertex.uv;
    out.tile_coord_start = tile_coord_start;
    out.world_normal = vertex.normal;
    out.world_pos = (mesh.model * position).xyz;

    return out;
}

struct FragmentInput {
    @location(0) light_intensity: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tile_coord_start: vec2<f32>,
    @location(3) world_normal: vec3<f32>,
    @location(4) world_pos: vec3<f32>,
};

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let d = length(in.world_pos - view.world_position);

    let tiled_coord = in.uv % material_data.tile_texture_size;
    var color = textureSample(atlas_texture, atlas_sampler, in.tile_coord_start + tiled_coord);

    color.a = 1.0;//clamp(d, 0.0, 1.0);

    return color * vec4<f32>(in.light_intensity, 1.0);
}