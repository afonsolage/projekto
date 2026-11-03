use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use futures_lite::future::block_on;
use projekto_archive::{
    Archive, ChunkAsset, HEADER_SIZE, SECTOR_INDEX_SIZE, SECTOR_SIZE, VERSION_MAGIC,
};
use projekto_core::{
    chunk,
    coords::{Chunk, ChunkVoxel, Region, RegionChunk},
};
use serde::Serialize;
use serde_json::json;

const REGION_AXIS: i32 = Region::AXIS_SIZE as i32;

fn main() {
    if let Err(error) = run(std::env::args().skip(1).collect()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };

    match command {
        "info" => info(required_path(&args, 1)?, flag(&args, "--json")),
        "list" => list(
            required_path(&args, 1)?,
            flag(&args, "--chunks"),
            flag(&args, "--json"),
        ),
        "holes" => holes(
            required_path(&args, 1)?,
            parse_center(&args)?,
            value_of(&args, "--radius").and_then(|value| value.parse().ok()),
            value_of(&args, "--limit").and_then(|value| value.parse().ok()),
            flag(&args, "--json"),
        ),
        "verify" => verify(required_path(&args, 1)?, flag(&args, "--json")),
        "dump" => dump(
            required_path(&args, 1)?,
            parse_i32(&args, 2)?,
            parse_i32(&args, 3)?,
            flag(&args, "--map"),
            flag(&args, "--json"),
        ),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown command `{other}`; run `projekto-archive help`").into()),
    }
}

fn print_help() {
    println!(
        "\
projekto-archive — inspect Projekto region archives

USAGE:
  projekto-archive info   <region-dir> [--json]
  projekto-archive list   <region-dir> [--chunks] [--json]
  projekto-archive holes  <region-dir> [--center x,z] [--radius N] [--limit N] [--json]
  projekto-archive verify <region-dir> [--json]
  projekto-archive dump   <region-dir> <chunk-x> <chunk-z> [--map] [--json]

The region directory is usually `archive/region`."
    );
}

fn required_path(args: &[String], index: usize) -> Result<&Path, Box<dyn Error>> {
    args.get(index)
        .map(Path::new)
        .ok_or_else(|| "missing region directory".into())
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|arg| arg == name)
}

fn value_of<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn parse_center(args: &[String]) -> Result<Option<(i32, i32)>, Box<dyn Error>> {
    let Some(value) = value_of(args, "--center") else {
        return Ok(None);
    };
    let (x, z) = value
        .split_once(',')
        .ok_or_else(|| "--center must be formatted as x,z".to_string())?;
    Ok(Some((x.parse()?, z.parse()?)))
}

fn parse_i32(args: &[String], index: usize) -> Result<i32, Box<dyn Error>> {
    args.get(index)
        .ok_or_else(|| format!("missing argument {index}").into())
        .and_then(|value| {
            value
                .parse::<i32>()
                .map_err(|_| format!("`{value}` is not a valid integer").into())
        })
}

#[derive(Serialize)]
struct RegionInfo {
    x: i32,
    z: i32,
    file: String,
    bytes: u64,
    indexed: usize,
    used_sectors: u64,
    orphan_sectors: u64,
}

fn info(dir: &Path, as_json: bool) -> Result<(), Box<dyn Error>> {
    let files = region_files(dir)?;
    let mut regions = Vec::new();
    let mut total_bytes = 0u64;
    let mut total_indexed = 0usize;
    let mut total_used_sectors = 0u64;

    for (region, path) in files {
        let len = fs::metadata(&path)?.len();
        let header = read_header(&path)?;
        let indexed = header.iter().filter(|entry| entry.offset != 0).count();
        let used_sectors: u64 = header.iter().map(|entry| entry.sectors as u64).sum();
        let file_sectors = len / SECTOR_SIZE as u64;
        let orphan_sectors = file_sectors.saturating_sub(1 + used_sectors);

        total_bytes += len;
        total_indexed += indexed;
        total_used_sectors += used_sectors;

        regions.push(RegionInfo {
            x: region.x,
            z: region.z,
            file: path.display().to_string(),
            bytes: len,
            indexed,
            used_sectors,
            orphan_sectors,
        });
    }

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "format_version": VERSION_MAGIC,
                "sector_size": SECTOR_SIZE,
                "regions": regions,
                "total_chunks": total_indexed,
                "total_bytes": total_bytes,
                "total_used_sectors": total_used_sectors,
            }))?
        );
        return Ok(());
    }

    println!("format version: {VERSION_MAGIC:02X?}");
    println!("sector size: {SECTOR_SIZE} B");
    for region in regions {
        println!(
            "region ({}, {}): {} ({} B), indexed {}/{}, used {} sectors, orphan {} sectors",
            region.x,
            region.z,
            region.file,
            region.bytes,
            region.indexed,
            Region::BUFFER_SIZE,
            region.used_sectors,
            region.orphan_sectors,
        );
    }
    println!("total: {total_indexed} chunks, {total_bytes} B, {total_used_sectors} used sectors");
    Ok(())
}

fn list(dir: &Path, show_chunks: bool, as_json: bool) -> Result<(), Box<dyn Error>> {
    let mut chunks = 0usize;
    let mut vertices = 0usize;
    let mut suspicious = 0usize;

    let summary = scan_chunks(dir, |chunk, asset| {
        chunks += 1;
        vertices += asset.vertex.len();

        if asset.vertex.is_empty() {
            let report = ChunkReport::new(chunk, &asset);
            if report.suspicious {
                suspicious += 1;
            }

            if show_chunks {
                println!("{report}");
            }
        } else if show_chunks {
            println!("chunk {chunk}: vertices={}", asset.vertex.len());
        }
    })?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "chunks": chunks,
                "vertices": vertices,
                "suspicious_empty_meshes": suspicious,
                "read_errors": summary.errors,
            }))?
        );
    } else {
        println!("chunks: {chunks}, vertices: {vertices}, suspicious empty meshes: {suspicious}");
    }

    if summary.errors > 0 {
        return Err(format!("{} archive entries failed to read", summary.errors).into());
    }
    Ok(())
}

fn holes(
    dir: &Path,
    center: Option<(i32, i32)>,
    radius: Option<i32>,
    limit: Option<usize>,
    as_json: bool,
) -> Result<(), Box<dyn Error>> {
    let mut chunks = 0usize;
    let mut reports = Vec::new();

    let summary = scan_chunks(dir, |chunk, asset| {
        chunks += 1;

        if !asset.vertex.is_empty() {
            return;
        }

        if let Some((cx, cz)) = center
            && let Some(radius) = radius
            && ((chunk.x - cx).abs() > radius || (chunk.z - cz).abs() > radius)
        {
            return;
        }

        let report = ChunkReport::new(chunk, &asset);
        if report.suspicious && limit.is_none_or(|limit| reports.len() < limit) {
            reports.push(report);
        }
    })?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
    } else {
        for report in &reports {
            println!("{report}");
        }
        println!(
            "scanned {chunks} chunks, found {} suspicious empty meshes",
            reports.len()
        );
    }

    if summary.errors > 0 {
        return Err(format!("{} archive entries failed to read", summary.errors).into());
    }
    Ok(())
}

fn verify(dir: &Path, as_json: bool) -> Result<(), Box<dyn Error>> {
    let mut empty_surface = 0usize;

    let summary = scan_chunks(dir, |chunk, asset| {
        if asset.vertex.is_empty() {
            let report = ChunkReport::new(chunk, &asset);
            if report.suspicious {
                empty_surface += 1;
            }
        }
    })?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "chunks": summary.chunks,
                "suspicious_empty_meshes": empty_surface,
                "read_errors": summary.errors,
            }))?
        );
    } else {
        println!(
            "verified {} chunks, {empty_surface} suspicious empty meshes, {} read errors",
            summary.chunks, summary.errors
        );
    }

    if summary.errors > 0 {
        return Err(format!("{} archive entries failed to read", summary.errors).into());
    }

    if empty_surface > 0 {
        return Err(format!(
            "{empty_surface} chunks have a surface but no saved mesh; run `projekto-archive holes` for details"
        )
        .into());
    }

    Ok(())
}

fn dump(dir: &Path, x: i32, z: i32, show_map: bool, as_json: bool) -> Result<(), Box<dyn Error>> {
    let chunk = Chunk::new(x, z);
    let region = Region::from_chunk(chunk);
    let path = region_path(dir, region);

    if !path.exists() {
        return Err(format!("region file not found: {}", path.display()).into());
    }

    let region_chunk = RegionChunk::from_chunk(chunk);
    let mut archive = open_archive(&path)?;
    let asset = block_on(archive.read(region_chunk))?;

    let Some(asset) = asset else {
        return Err(format!("chunk {chunk} is not indexed in {}", path.display()).into());
    };

    let report = ChunkReport::new(chunk, &asset);
    let heights = show_map.then(|| top_heights(&asset));

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "chunk": { "x": x, "z": z },
                "region": { "x": region.x, "z": region.z },
                "local": { "x": region_chunk.x, "z": region_chunk.z },
                "file": path.display().to_string(),
                "vertices": asset.vertex.len(),
                "faces": asset.vertex.len() / 4,
                "report": report,
                "top_heights": heights,
            }))?
        );
        return Ok(());
    }

    println!("chunk: {chunk}");
    println!("region: {region}, local: {region_chunk}");
    println!("region file: {}", path.display());
    println!("vertices: {}", asset.vertex.len());
    println!("faces: {}", asset.vertex.len() / 4);
    println!("{report}");

    if let Some(heights) = heights {
        println!("\ntop solid height per column (y or --):");
        for row in heights {
            let line = row
                .iter()
                .map(|height| match height {
                    Some(y) => format!("{y:>3}"),
                    None => "  -".to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ");
            println!("{line}");
        }
    }

    Ok(())
}

#[derive(Default)]
struct ScanSummary {
    chunks: usize,
    errors: usize,
}

fn scan_chunks(
    dir: &Path,
    mut callback: impl FnMut(Chunk, ChunkAsset),
) -> Result<ScanSummary, Box<dyn Error>> {
    let mut summary = ScanSummary::default();

    for (region, path) in region_files(dir)? {
        let header = read_header(&path)?;
        let mut archive = open_archive(&path)?;

        for (index, entry) in header.iter().enumerate() {
            if entry.offset == 0 {
                continue;
            }

            let local_x = (index >> 5) as u8;
            let local_z = (index & 0x1f) as u8;
            let chunk = Chunk::new(
                region.x * REGION_AXIS + local_x as i32,
                region.z * REGION_AXIS + local_z as i32,
            );

            match block_on(archive.read(RegionChunk::new(local_x, local_z))) {
                Ok(Some(asset)) => {
                    if asset.chunk != chunk {
                        eprintln!(
                            "warning: chunk {chunk} in {} contains chunk {} in its payload",
                            path.display(),
                            asset.chunk
                        );
                    }

                    summary.chunks += 1;
                    callback(chunk, asset);
                }
                Ok(None) => {
                    summary.errors += 1;
                    eprintln!(
                        "warning: chunk {chunk} is indexed in {} but returned no data",
                        path.display()
                    );
                }
                Err(error) => {
                    summary.errors += 1;
                    eprintln!(
                        "error: failed to read chunk {chunk} from {}: {error}",
                        path.display()
                    );
                }
            }
        }
    }

    Ok(summary)
}

fn top_heights(asset: &ChunkAsset) -> [[Option<u8>; Chunk::Z_AXIS_SIZE]; Chunk::X_AXIS_SIZE] {
    let mut heights = [[None; Chunk::Z_AXIS_SIZE]; Chunk::X_AXIS_SIZE];

    for (x, row) in heights.iter_mut().enumerate() {
        for (z, height) in row.iter_mut().enumerate() {
            for y in (0..Chunk::Y_AXIS_SIZE).rev() {
                let voxel = ChunkVoxel::new(x as u8, y as u8, z as u8);
                if !asset.kind.get(voxel).is_none() {
                    *height = Some(y as u8);
                    break;
                }
            }
        }
    }

    heights
}

fn region_files(dir: &Path) -> Result<Vec<(Region, PathBuf)>, Box<dyn Error>> {
    let mut files = BTreeMap::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rgn") {
            continue;
        }

        let Some(region) = parse_region_name(&path) else {
            eprintln!(
                "warning: skipping unrecognized region file {}",
                path.display()
            );
            continue;
        };

        files.insert((region.x, region.z), (region, path));
    }

    Ok(files.into_values().collect())
}

fn parse_region_name(path: &Path) -> Option<Region> {
    let stem = path.file_stem()?.to_str()?;
    let (x, z) = stem.split_once('_')?;
    Some(Region::new(x.parse().ok()?, z.parse().ok()?))
}

fn region_path(dir: &Path, region: Region) -> PathBuf {
    dir.join(format!("{}_{}.rgn", region.x, region.z))
}

fn open_archive(path: &Path) -> Result<Archive<ChunkAsset>, Box<dyn Error>> {
    let path = path
        .to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))?;
    Ok(block_on(Archive::new(path))?)
}

#[derive(Clone, Copy, Debug)]
struct HeaderEntry {
    offset: u16,
    sectors: u16,
}

fn read_header(path: &Path) -> Result<Vec<HeaderEntry>, Box<dyn Error>> {
    let mut file = fs::File::open(path)?;
    let mut bytes = vec![0u8; HEADER_SIZE];
    file.read_exact(&mut bytes)
        .map_err(|error| format!("{}: cannot read archive header: {error}", path.display()))?;

    Ok(bytes
        .as_chunks::<SECTOR_INDEX_SIZE>()
        .0
        .iter()
        .map(|entry| {
            let value = u32::from_be_bytes(*entry);
            HeaderEntry {
                offset: (value >> 16) as u16,
                sectors: (value & 0xffff) as u16,
            }
        })
        .collect())
}

#[derive(Serialize)]
struct ChunkReport {
    chunk: Chunk,
    vertices: usize,
    solid_voxels: u32,
    surface_voxels: u32,
    suspicious: bool,
}

impl ChunkReport {
    fn new(chunk: Chunk, asset: &ChunkAsset) -> Self {
        let mut solid_voxels = 0;
        let mut surface_voxels = 0;

        for voxel in chunk::voxels() {
            if asset.kind.get(voxel).is_none() {
                continue;
            }

            solid_voxels += 1;

            let exposed = voxel.y == Chunk::Y_END
                || asset
                    .kind
                    .get(ChunkVoxel::new(voxel.x, voxel.y + 1, voxel.z))
                    .is_none();

            if exposed {
                surface_voxels += 1;
            }
        }

        Self {
            chunk,
            vertices: asset.vertex.len(),
            solid_voxels,
            surface_voxels,
            suspicious: asset.lod == 0 && asset.vertex.is_empty() && surface_voxels > 0,
        }
    }
}

impl std::fmt::Display for ChunkReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "chunk {}: vertices={} solid={} surface={}{}",
            self.chunk,
            self.vertices,
            self.solid_voxels,
            self.surface_voxels,
            if self.suspicious {
                " [SUSPICIOUS: empty mesh on a surface chunk]"
            } else {
                ""
            }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_asset() -> ChunkAsset {
        let mut asset = ChunkAsset::default();
        asset
            .kind
            .set(ChunkVoxel::new(0, 0, 0), projekto_core::voxel::Kind::id(1));
        asset.kind.pack();
        asset
    }

    #[test]
    fn parses_region_names() {
        assert_eq!(
            parse_region_name(Path::new("-1_2.rgn")).map(|region| (region.x, region.z)),
            Some((-1, 2))
        );
        assert_eq!(parse_region_name(Path::new("not-a-region.rgn")), None);
    }

    #[test]
    fn reads_header_entries() {
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes[..4].copy_from_slice(&((1u32 << 16) | 3).to_be_bytes());
        bytes[4..8].copy_from_slice(&((5u32 << 16) | 7).to_be_bytes());

        let path = temp_path("header");
        fs::write(&path, &bytes).unwrap();
        let header = read_header(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(header[0].offset, 1);
        assert_eq!(header[0].sectors, 3);
        assert_eq!(header[1].offset, 5);
        assert_eq!(header[1].sectors, 7);
    }

    #[test]
    fn flags_empty_surface_chunks() {
        let chunk = Chunk::new(0, 0);
        let report = ChunkReport::new(chunk, &solid_asset());
        assert!(report.suspicious);

        let mut with_vertex = solid_asset();
        with_vertex.vertex.push(Default::default());
        assert!(!ChunkReport::new(chunk, &with_vertex).suspicious);

        let mut coarse_empty = solid_asset();
        coarse_empty.lod = 1;
        assert!(!ChunkReport::new(chunk, &coarse_empty).suspicious);
    }

    #[test]
    fn does_not_flag_empty_chunks() {
        let asset = ChunkAsset::default();
        assert!(!ChunkReport::new(Chunk::new(0, 0), &asset).suspicious);
    }

    #[test]
    fn reads_a_saved_chunk() {
        let dir = temp_dir("read_saved");
        fs::create_dir_all(&dir).unwrap();
        let path = region_path(&dir, Region::new(0, 0));

        let chunk = Chunk::new(3, 4);
        let mut asset = solid_asset();
        asset.chunk = chunk;
        let data = Archive::<ChunkAsset>::compress(&asset).unwrap();

        let mut archive = block_on(Archive::<ChunkAsset>::new(path.to_str().unwrap())).unwrap();
        block_on(archive.write(RegionChunk::from_chunk(chunk), data)).unwrap();
        block_on(archive.save_header()).unwrap();
        drop(archive);

        let mut found = None;
        scan_chunks(&dir, |scanned, asset| {
            if scanned == chunk {
                found = Some(asset);
            }
        })
        .unwrap();

        let _ = fs::remove_dir_all(&dir);

        let found = found.expect("saved chunk should be found");
        assert_eq!(found.chunk, asset.chunk);
        assert_eq!(found.kind, asset.kind);
    }

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "projekto_archive_cli_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir = temp_dir(name);
        fs::create_dir_all(&dir).unwrap();
        dir.join("test.rgn")
    }
}
