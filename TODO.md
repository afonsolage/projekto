```markdown
# Projekto – Sub-Chunk + Discrete-LOD Roadmap  
*(Living document – tick items as you finish them)*

> **Goal:** 16³ compressed sub-chunks + discrete LOD meshes cached on disk.  
> 48-bit Morton X-Z key + 16-bit Y → 65 536 directories max.

---

## ✅ Milestone 0 – House-Keeping
- [ ] Create / overwrite `ROADMAP.md` and link in `README.md`.  
- [ ] Move old `Chunk` / `ChunkStorage` to `src/world/chunk_legacy.rs`; mark `#[deprecated]`.

---

## 🧱 Milestone 1 – Sub-Chunk Data Model
- [ ] Define `SubChunkKey(IVec3)` in `src/world/sc.rs`.  
- [ ] Implement `CompressedBlocks` enum (`Single`, `Bits2`, `Bits8`).  
- [ ] Write round-trip unit tests: compress → decompress → assert identical.  
- [ ] Add `MeshStamp { data_hash: u64, lod: u8 }` component.  
- [ ] Add `bincode + zstd` serialisation; bench < 0.1 ms / SC.

---

## 🗃️ Milestone 2 – Persistence Layer
- [ ] Create `ScStore` resource (flat file, sharded) in `src/world/sc_store.rs`.  
- [ ] Implement `load_sc(key) -> Option<CompressedBlocks>` and `save_sc`.  
- [ ] Async wrapper with `bevy_tasks::IoTaskPool`.  
- [ ] Add 512-entry LRU in memory.

---

## 🏗️ Milestone 3 – Discrete-LOD Cache
- [ ] Define enum `enum Lod { L0 = 0, L1 = 2, L2 = 4 }` (step sizes).  
- [ ] Implement `mesh_lod(sc: &CompressedBlocks, lod: Lod) -> Mesh`.  
- [ ] Store meshes on disk:  
  `cache/meshes/{XXXX}/{YYYYYYYYYYYY}.l0.mesh.zst`  
  `cache/meshes/{XXXX}/{YYYYYYYYYYYY}.l1.mesh.zst`  
  `cache/meshes/{XXXX}/{YYYYYYYYYYYY}.l2.mesh.zst`  
- [ ] Add `MeshLod` component (`lod: Lod`) for live entities.  
- [ ] Distance-selector system: choose `Lod` from camera distance.

---

## 🧩 Milestone 4 – ECS Integration
- [ ] Replace old queries with `Query<(&SubChunkKey, &SubChunk, Option<&MeshStamp>)>`.  
- [ ] Add `sc_spawner.rs`: spawn / despawn SC within 16-SC radius.  
- [ ] Add `on_remove` hook to flush dirty SC to disk.

---

## 🔥 Milestone 5 – Gameplay Systems
- [ ] Lighting system (reads `kind`).  
- [ ] Fire propagation (reads/writes `temp`).  
- [ ] Structural integrity (reads/writes `integrity`).  
- [ ] Emit `BlockChanged` / `MeshDirty` events.

---

## 🧪 Milestone 6 – Stress & Memory Profiling
- [ ] Run `valgrind --tool=massif` or `cargo flamegraph`; target < 300 MB RSS.  
- [ ] Add debug overlay:  
  `SC loaded / SC cached / meshes cached / RAM MB`.

---

## 🚀 Milestone 7 – Polish & Release 0.1
- [ ] Add save-on-quit & load-on-join RPC for multiplayer.  
- [ ] Write migration tool `migrate_legacy_chunks.rs` (one-off).  
- [ ] Tag release `v0.1-sc-migration`, push to GitHub.

---

### 📁 File / Module Cheat-Sheet
| Path | Purpose |
|---|---|
| `src/world/sc.rs` | `SubChunkKey`, `CompressedBlocks` |
| `src/world/sc_store.rs` | `ScStore` resource (flat file, sharded) |
| `src/systems/sc_spawner.rs` | Spawn / despawn SC entities |
| `src/systems/meshing.rs` | Build & cache LOD meshes |
| `cache/meshes/` | Disk cache for LOD meshes |
```
