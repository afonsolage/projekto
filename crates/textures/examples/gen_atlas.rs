//! Command line example that writes a generated atlas and its individual tiles as PNGs.
//!
//! ```text
//! cargo run -p projekto_textures --example gen_atlas -- \
//!     --style structural --art warm --seed 42 \
//!     --out target/atlas.png --dump-tiles target/tiles
//! ```
//!
//! Generation takes no image input: the only inputs are the style, the art preset and the world
//! seed. The `image` crate is only used here, to save the example output.

use std::path::{Path, PathBuf};

use projekto_textures::{
    ATLAS_SIZE, TextureStyle, art::ArtPreset, generate_atlas, index_tile::TILE,
};

fn main() -> Result<(), String> {
    let options = Options::parse(std::env::args().skip(1))?;

    let started = std::time::Instant::now();
    let atlas = generate_atlas(options.style, options.art, options.seed);
    let elapsed = started.elapsed();
    save_png(&options.out, &atlas.rgba, ATLAS_SIZE, ATLAS_SIZE)?;
    println!(
        "wrote {} (style={}, art={}, seed={}, {} bytes, generated in {:.2} ms)",
        options.out.display(),
        atlas.style.name(),
        atlas.art.name(),
        atlas.seed,
        atlas.rgba.len(),
        elapsed.as_secs_f64() * 1000.0
    );

    if let Some(directory) = &options.dump_tiles {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
        for tile in &atlas.tiles {
            let path = directory.join(format!("{}.png", tile.name));
            save_png(&path, &tile.rgba, TILE, TILE)?;
            println!("wrote {}", path.display());
        }
    }

    Ok(())
}

struct Options {
    style: TextureStyle,
    art: ArtPreset,
    seed: u64,
    out: PathBuf,
    dump_tiles: Option<PathBuf>,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            style: TextureStyle::default(),
            art: ArtPreset::default(),
            seed: 42,
            out: PathBuf::from("target/atlas.png"),
            dump_tiles: None,
        };

        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--style" => {
                    let value = args.next().ok_or("--style needs a value")?;
                    options.style = TextureStyle::parse(&value)
                        .ok_or_else(|| format!("unknown style {value}"))?;
                }
                "--art" => {
                    let value = args.next().ok_or("--art needs a value")?;
                    options.art = ArtPreset::parse(&value)
                        .ok_or_else(|| format!("unknown art preset {value}"))?;
                }
                "--seed" => {
                    let value = args.next().ok_or("--seed needs a value")?;
                    options.seed = value.parse().map_err(|_| format!("invalid seed {value}"))?;
                }
                "--out" => {
                    let value = args.next().ok_or("--out needs a value")?;
                    options.out = PathBuf::from(value);
                }
                "--dump-tiles" => {
                    let value = args.next().ok_or("--dump-tiles needs a value")?;
                    options.dump_tiles = Some(PathBuf::from(value));
                }
                "--help" | "-h" => return Err(help()),
                other => return Err(format!("unknown argument {other}\n\n{}", help())),
            }
        }

        if let Some(parent) = options.out.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }

        Ok(options)
    }
}

fn save_png(path: &Path, rgba: &[u8], width: usize, height: usize) -> Result<(), String> {
    image::save_buffer_with_format(
        path,
        rgba,
        width as u32,
        height as u32,
        image::ExtendedColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn help() -> String {
    let styles = TextureStyle::ALL
        .iter()
        .map(|style| style.name())
        .collect::<Vec<_>>()
        .join(", ");
    let arts = ArtPreset::ALL
        .iter()
        .map(|art| art.name())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "usage: gen_atlas [--style <{styles}>] [--art <{arts}>] [--seed <u64>] [--out <path>] [--dump-tiles <dir>]"
    )
}
