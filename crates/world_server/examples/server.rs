use projekto_world_server::set::Landscape;

fn main() {
    let mut app = projekto_world_server::app::create();
    app.insert_resource(Landscape {
        radius: 32,
        ..Default::default()
    })
    .run();
}

const fn test() -> usize {
    const A: [usize; 5] = [10, 20, 30, 40, 50];

    let mut i = 0;
    let mut max = 0;
    while i < A.len() {
        if A[i] > max {
            max = A[i];
        }
        i += 1;
    }

    max
}
