use projekto_server::{
    ChunkAsset,
    meshing::{generate_vertices, greedy},
};

fn main() {
    let ChunkAsset {
        kind,
        light,
        occlusion,
        soft_light,
        ..
    } = setup();

    println!("Kind: {kind:?}");
    println!("Light: {light:?}");
    println!("Occlusion: {occlusion:?}");
    println!("Soft Light: {soft_light:?}");

    let greedy = greedy::generate_faces(&kind, &occlusion, &soft_light);
    let vertices_greedy = generate_vertices(&greedy);

    println!("Greedy: {:?}", greedy.len());
    println!("Vertices Greedy: {:?}", vertices_greedy.len());
}

fn setup() -> ChunkAsset {
    let path = std::path::Path::new("chunks/0_0");
    let bytes = std::fs::read(path).unwrap();
    let (asset, _) =
        bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    asset
}
