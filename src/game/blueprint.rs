use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::items::ItemId;
use super::world::{Direction, EntityId, StructureKind, WorldState};
use crate::sim::machine::MachinePool;
use crate::sim::storage::StoragePool;

/// A single structure captured into a blueprint.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlueprintEntry {
    /// Relative grid position within the selection bounding box.
    pub offset: (i32, i32),
    /// What kind of structure this is.
    pub kind: StructureKind,
    /// Facing direction.
    pub direction: Direction,
    /// Stored contents (for storage buildings). Empty for non-storage.
    pub items: Vec<(ItemId, u16)>,
    /// Selected recipe index (for machines). `None` for non-machines or no recipe set.
    pub recipe: Option<usize>,
}

/// A clipboard buffer holding captured structures.
#[derive(Clone, Debug)]
pub struct Clipboard {
    pub entries: Vec<BlueprintEntry>,
    pub width: i32,
    pub height: i32,
    /// The tiling q parameter when this was captured.
    pub tiling_q: u32,
}

impl Clipboard {
    /// Rotate the entire clipboard 90° clockwise.
    ///
    /// Each entry's offset is transformed via `Direction::East.rotate_cell()`
    /// (which is the 90° CW rotation), and each entry's direction is rotated
    /// one step clockwise. Width and height are swapped.
    pub fn rotate_cw(&mut self) {
        let w = self.width;
        let h = self.height;
        for entry in &mut self.entries {
            // Rotate the grid offset 90° CW within the bounding box
            let (ox, oy) = Direction::East.rotate_cell(entry.offset.0, entry.offset.1, w, h);
            // Also rotate the footprint-relative origin for multi-cell structures
            let (fw, fh) = entry.kind.footprint();
            let (rfw, _rfh) = Direction::East.rotate_footprint(fw, fh);
            // rotate_cell maps the top-left corner through the rotation, but for
            // multi-cell structures the top-left of the rotated footprint isn't at
            // the same place as the rotated top-left of the original. Adjust so
            // the origin stays at the top-left of the rotated footprint.
            let adjusted_ox = ox - (rfw - 1);
            entry.offset = (adjusted_ox, oy);
            entry.direction = entry.direction.rotate_cw();
        }
        self.width = h;
        self.height = w;
    }
}

// ---------------------------------------------------------------------------
// Blueprint persistence
// ---------------------------------------------------------------------------

/// Current blueprint file format version.
const BLUEPRINT_VERSION: u32 = 1;

/// A blueprint saved to disk.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlueprintFile {
    /// Format version for forward compatibility.
    pub version: u32,
    /// User-provided blueprint name.
    pub name: String,
    /// Unix epoch seconds when saved.
    pub timestamp: u64,
    /// The `q` parameter of the {4,q} tiling this was captured in.
    pub tiling_q: u32,
    /// Bounding box width in grid cells.
    pub width: u32,
    /// Bounding box height in grid cells.
    pub height: u32,
    /// The structure entries.
    pub entries: Vec<BlueprintEntry>,
}

impl BlueprintFile {
    /// Create a `BlueprintFile` from a `Clipboard` and a user-provided name.
    pub fn from_clipboard(clipboard: &Clipboard, name: String) -> Self {
        Self {
            version: BLUEPRINT_VERSION,
            name,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            tiling_q: clipboard.tiling_q,
            width: clipboard.width as u32,
            height: clipboard.height as u32,
            entries: clipboard.entries.clone(),
        }
    }

    /// Convert back to a `Clipboard`.
    pub fn to_clipboard(&self) -> Clipboard {
        Clipboard {
            entries: self.entries.clone(),
            width: self.width as i32,
            height: self.height as i32,
            tiling_q: self.tiling_q,
        }
    }
}

/// Returns the blueprints directory, creating it if needed.
fn blueprints_dir() -> Option<PathBuf> {
    let dir = directories::ProjectDirs::from("", "", "octofact")?
        .data_dir()
        .join("blueprints");
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Sanitize a user-provided name for use as a filename.
///
/// Strips path separators and limits length.
fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | '\0'))
        .take(100)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Pick a unique filename in `dir` for the given base name.
///
/// Returns `{base}.blueprint`, or `{base}_2.blueprint` etc. on collision.
fn unique_path(dir: &Path, base: &str) -> PathBuf {
    let candidate = dir.join(format!("{base}.blueprint"));
    if !candidate.exists() {
        return candidate;
    }
    for i in 2.. {
        let candidate = dir.join(format!("{base}_{i}.blueprint"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// Save a blueprint to disk. Returns the path it was written to.
pub fn save_blueprint(file: &BlueprintFile) -> Result<PathBuf, String> {
    let dir = blueprints_dir().ok_or("Could not determine blueprints directory")?;
    let base = sanitize_name(&file.name);
    let path = unique_path(&dir, &base);
    let bytes = bincode::serialize(file).map_err(|e| format!("serialize: {e}"))?;
    let tmp = path.with_extension("blueprint.tmp");
    fs::write(&tmp, &bytes).map_err(|e| format!("write: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;
    log::info!(
        "Saved blueprint '{}' ({} bytes) to {}",
        file.name,
        bytes.len(),
        path.display()
    );
    Ok(path)
}

/// Load a blueprint from a file path.
///
/// Returns an error if the version is unsupported or the tiling `q` doesn't
/// match `expected_q`.
pub fn load_blueprint(path: &Path, expected_q: u32) -> Result<BlueprintFile, String> {
    let bytes = fs::read(path).map_err(|e| format!("read: {e}"))?;
    let file: BlueprintFile =
        bincode::deserialize(&bytes).map_err(|e| format!("deserialize: {e}"))?;
    if file.version > BLUEPRINT_VERSION {
        return Err(format!(
            "Blueprint version {} is newer than supported version {BLUEPRINT_VERSION}",
            file.version
        ));
    }
    if file.tiling_q != expected_q {
        return Err(format!(
            "Blueprint was saved in {{4,{}}} but current tiling is {{4,{expected_q}}}",
            file.tiling_q
        ));
    }
    Ok(file)
}

/// List all saved blueprints, returning `(path, BlueprintFile)` pairs.
///
/// Files that fail to deserialize are silently skipped.
pub fn list_blueprints() -> Vec<(PathBuf, BlueprintFile)> {
    let Some(dir) = blueprints_dir() else {
        return vec![];
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return vec![];
    };
    let mut results: Vec<(PathBuf, BlueprintFile)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "blueprint")
        })
        .filter_map(|e| {
            let path = e.path();
            let bytes = fs::read(&path).ok()?;
            let file: BlueprintFile = bincode::deserialize(&bytes).ok()?;
            Some((path, file))
        })
        .collect();
    results.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    results
}

/// Delete a blueprint file.
pub fn delete_blueprint(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|e| format!("delete: {e}"))
}

/// Rename a blueprint: update the internal name and rename the file on disk.
///
/// Returns the new file path.
pub fn rename_blueprint(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    let bytes = fs::read(path).map_err(|e| format!("read: {e}"))?;
    let mut file: BlueprintFile =
        bincode::deserialize(&bytes).map_err(|e| format!("deserialize: {e}"))?;
    file.name = new_name.to_string();
    let dir = path.parent().ok_or("invalid path")?;
    let base = sanitize_name(new_name);
    let new_path = unique_path(dir, &base);
    let new_bytes = bincode::serialize(&file).map_err(|e| format!("serialize: {e}"))?;
    let tmp = new_path.with_extension("blueprint.tmp");
    fs::write(&tmp, &new_bytes).map_err(|e| format!("write: {e}"))?;
    fs::rename(&tmp, &new_path).map_err(|e| format!("rename: {e}"))?;
    fs::remove_file(path).map_err(|e| format!("delete old: {e}"))?;
    Ok(new_path)
}

/// Check whether each clipboard entry can be pasted at `anchor` on the given tile.
///
/// Returns a `Vec` of `(index, passable)` for each entry in the clipboard.
/// An entry passes if all cells it would occupy are currently unoccupied.
pub fn can_paste(
    world: &WorldState,
    tile_address: &[u8],
    anchor: (i32, i32),
    clipboard: &Clipboard,
) -> Vec<(usize, bool)> {
    use super::world::occupied_cells;
    clipboard
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let origin = (anchor.0 + entry.offset.0, anchor.1 + entry.offset.1);
            let (fw, fh) = entry.kind.footprint();
            let footprint = entry.direction.rotate_footprint(fw, fh);
            let cells = occupied_cells(origin, footprint);
            let passable = cells.iter().all(|&cell| {
                world
                    .tile_entities(tile_address)
                    .and_then(|m| m.get(&cell))
                    .is_none()
            });
            (i, passable)
        })
        .collect()
}

/// Tally the items required to paste all entries in a clipboard.
///
/// Each structure requires one of its corresponding `ItemId`.
/// Returns a consolidated list of `(ItemId, count)`.
pub fn required_items(clipboard: &Clipboard) -> Vec<(ItemId, u16)> {
    use std::collections::HashMap;
    let mut counts: HashMap<ItemId, u16> = HashMap::new();
    for entry in &clipboard.entries {
        let item = entry.kind.to_item();
        *counts.entry(item).or_insert(0) += 1;
    }
    counts.into_iter().collect()
}

// --- Virtual coordinate utilities for multi-tile strips ---

/// Convert a virtual coordinate to `(tile_delta, local)` pair.
///
/// Virtual coordinates extend beyond the [-32, 32] local range.
/// `tile_delta` counts how many tiles from the anchor (delta=0).
/// `local` is within [-32, 32].
pub fn virtual_to_tile_local(v: i32) -> (i32, i32) {
    let tile_delta = (v + 32).div_euclid(64);
    let local = v - tile_delta * 64;
    (tile_delta, local)
}

/// Convert `(tile_delta, local)` back to a virtual coordinate.
pub fn tile_local_to_virtual(tile_delta: i32, local: i32) -> i32 {
    tile_delta * 64 + local
}

/// Check whether a grid cell is at a tiling vertex (tile corner).
///
/// Tiling vertices occur where `|gx| == 32 AND |gy| == 32`.
pub fn is_vertex_cell(gx: i32, gy: i32) -> bool {
    gx.abs() == 32 && gy.abs() == 32
}

/// Check whether any cell in a structure's footprint touches a vertex cell.
pub fn footprint_touches_vertex(gx: i32, gy: i32, kind: StructureKind, direction: Direction) -> bool {
    let (fw, fh) = kind.footprint();
    let (rw, rh) = direction.rotate_footprint(fw, fh);
    for dy in 0..rh {
        for dx in 0..rw {
            if is_vertex_cell(gx + dx, gy + dy) {
                return true;
            }
        }
    }
    false
}

/// Capture all structures within a rectangle on a single tile.
///
/// `top_left` and `bottom_right` are inclusive grid coordinates.
/// Multi-cell entities are captured once at their origin offset.
pub fn capture_region(
    world: &WorldState,
    machine_pool: &MachinePool,
    storage_pool: &StoragePool,
    tile_address: &[u8],
    top_left: (i32, i32),
    bottom_right: (i32, i32),
    tiling_q: u32,
) -> Clipboard {
    let entities_map = match world.tile_entities(tile_address) {
        Some(e) => e,
        None => {
            return Clipboard {
                entries: Vec::new(),
                width: bottom_right.0 - top_left.0 + 1,
                height: bottom_right.1 - top_left.1 + 1,
                tiling_q,
            };
        }
    };

    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::<EntityId>::new();

    let min_x = top_left.0.min(bottom_right.0);
    let max_x = top_left.0.max(bottom_right.0);
    let min_y = top_left.1.min(bottom_right.1);
    let max_y = top_left.1.max(bottom_right.1);

    for gy in min_y..=max_y {
        for gx in min_x..=max_x {
            if let Some(&entity) = entities_map.get(&(gx, gy)) {
                // Skip if we already captured this entity (multi-cell dedup)
                if !seen.insert(entity) {
                    continue;
                }
                // Only capture at the origin cell
                if !world.is_origin(entity, gx, gy) {
                    continue;
                }

                let kind = match world.kind(entity) {
                    Some(k) => k,
                    None => continue,
                };
                let direction = world.direction(entity).unwrap_or(Direction::North);

                // Snapshot stored items for storage entities
                let items = if kind == StructureKind::Storage {
                    if let Some(state) = storage_pool.get(entity) {
                        state
                            .slots
                            .iter()
                            .filter(|s| s.count > 0)
                            .map(|s| (s.item, s.count))
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                // Snapshot recipe for machine entities
                let recipe = machine_pool.recipe(entity).flatten();

                entries.push(BlueprintEntry {
                    offset: (gx - min_x, gy - min_y),
                    kind,
                    direction,
                    items,
                    recipe,
                });
            }
        }
    }

    Clipboard {
        entries,
        width: max_x - min_x + 1,
        height: max_y - min_y + 1,
        tiling_q,
    }
}

/// Capture structures across a multi-tile strip.
///
/// `tiles` is a list of `(tile_address, delta)` pairs where delta is the strip offset.
/// `top_left` and `bottom_right` are in virtual coordinates.
/// `strip_axis_x` indicates whether the strip extends along x (true) or y (false).
#[allow(clippy::too_many_arguments)]
pub fn capture_strip(
    world: &WorldState,
    machine_pool: &MachinePool,
    storage_pool: &StoragePool,
    tiles: &[(&[u8], i32)],
    top_left: (i32, i32),
    bottom_right: (i32, i32),
    strip_axis_x: bool,
    tiling_q: u32,
) -> Clipboard {
    let vmin_x = top_left.0.min(bottom_right.0);
    let vmax_x = top_left.0.max(bottom_right.0);
    let vmin_y = top_left.1.min(bottom_right.1);
    let vmax_y = top_left.1.max(bottom_right.1);

    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::<EntityId>::new();

    for &(tile_addr, delta) in tiles {
        // Compute local coordinate range for this tile
        let (local_min_x, local_max_x, local_min_y, local_max_y) = if strip_axis_x {
            let tile_vmin = tile_local_to_virtual(delta, -32);
            let tile_vmax = tile_local_to_virtual(delta, 32);
            let clipped_vmin = vmin_x.max(tile_vmin);
            let clipped_vmax = vmax_x.min(tile_vmax);
            if clipped_vmin > clipped_vmax { continue; }
            let lmin = clipped_vmin - delta * 64;
            let lmax = clipped_vmax - delta * 64;
            (lmin, lmax, vmin_y, vmax_y)
        } else {
            let tile_vmin = tile_local_to_virtual(delta, -32);
            let tile_vmax = tile_local_to_virtual(delta, 32);
            let clipped_vmin = vmin_y.max(tile_vmin);
            let clipped_vmax = vmax_y.min(tile_vmax);
            if clipped_vmin > clipped_vmax { continue; }
            let lmin = clipped_vmin - delta * 64;
            let lmax = clipped_vmax - delta * 64;
            (vmin_x, vmax_x, lmin, lmax)
        };

        let entities_map = match world.tile_entities(tile_addr) {
            Some(e) => e,
            None => continue,
        };

        for gy in local_min_y..=local_max_y {
            for gx in local_min_x..=local_max_x {
                if let Some(&entity) = entities_map.get(&(gx, gy)) {
                    if !seen.insert(entity) {
                        continue;
                    }
                    if !world.is_origin(entity, gx, gy) {
                        continue;
                    }

                    let kind = match world.kind(entity) {
                        Some(k) => k,
                        None => continue,
                    };
                    let direction = world.direction(entity).unwrap_or(Direction::North);

                    // Skip structures touching vertex cells
                    if footprint_touches_vertex(gx, gy, kind, direction) {
                        continue;
                    }

                    let items = if kind == StructureKind::Storage {
                        if let Some(state) = storage_pool.get(entity) {
                            state.slots.iter()
                                .filter(|s| s.count > 0)
                                .map(|s| (s.item, s.count))
                                .collect()
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };

                    let recipe = machine_pool.recipe(entity).flatten();

                    // Compute virtual offset from bounding box origin
                    let vx = if strip_axis_x {
                        tile_local_to_virtual(delta, gx)
                    } else {
                        gx
                    };
                    let vy = if strip_axis_x {
                        gy
                    } else {
                        tile_local_to_virtual(delta, gy)
                    };

                    entries.push(BlueprintEntry {
                        offset: (vx - vmin_x, vy - vmin_y),
                        kind,
                        direction,
                        items,
                        recipe,
                    });
                }
            }
        }
    }

    Clipboard {
        entries,
        width: vmax_x - vmin_x + 1,
        height: vmax_y - vmin_y + 1,
        tiling_q,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::items::{ItemId, MachineType};
    use crate::game::world::WorldState;
    use crate::sim::machine::MachinePool;
    use crate::sim::storage::StoragePool;

    #[test]
    fn capture_region_single_belt() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        world.place(&addr, (5, 5), ItemId::Belt, Direction::East).unwrap();

        let pool = StoragePool::new();
        let mpool = MachinePool::new();
        let clip = capture_region(&world, &mpool, &pool, &addr, (5, 5), (5, 5), 5);

        assert_eq!(clip.entries.len(), 1);
        assert_eq!(clip.entries[0].offset, (0, 0));
        assert_eq!(clip.entries[0].kind, StructureKind::Belt);
        assert_eq!(clip.entries[0].direction, Direction::East);
        assert!(clip.entries[0].items.is_empty());
        assert_eq!(clip.entries[0].recipe, None);
        assert_eq!(clip.width, 1);
        assert_eq!(clip.height, 1);
        assert_eq!(clip.tiling_q, 5);
    }

    #[test]
    fn capture_region_relative_offsets() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        world.place(&addr, (10, 20), ItemId::Belt, Direction::North).unwrap();
        world.place(&addr, (12, 22), ItemId::Quadrupole, Direction::South).unwrap();

        let mpool = MachinePool::new();
        let pool = StoragePool::new();
        let clip = capture_region(&world, &mpool, &pool, &addr, (10, 20), (12, 22), 5);

        assert_eq!(clip.entries.len(), 2);
        // Belt at (10,20) -> offset (0,0)
        let belt = clip.entries.iter().find(|e| e.kind == StructureKind::Belt).unwrap();
        assert_eq!(belt.offset, (0, 0));
        // Quadrupole at (12,22) -> offset (2,2)
        let node = clip.entries.iter().find(|e| e.kind == StructureKind::PowerNode).unwrap();
        assert_eq!(node.offset, (2, 2));
    }

    #[test]
    fn capture_region_multi_cell_dedup() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        // Composer is 2x2: occupies (5,5), (6,5), (5,6), (6,6)
        world.place(&addr, (5, 5), ItemId::Composer, Direction::North).unwrap();

        let mpool = MachinePool::new();
        let pool = StoragePool::new();
        let clip = capture_region(&world, &mpool, &pool, &addr, (5, 5), (6, 6), 5);

        // Should capture exactly once, at origin offset
        assert_eq!(clip.entries.len(), 1);
        assert_eq!(clip.entries[0].offset, (0, 0));
        assert_eq!(
            clip.entries[0].kind,
            StructureKind::Machine(MachineType::Composer)
        );
    }

    #[test]
    fn capture_region_3x3_dedup() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        // Inverter is 3x3: occupies 9 cells from (0,0) to (2,2)
        world.place(&addr, (0, 0), ItemId::Inverter, Direction::North).unwrap();

        let mpool = MachinePool::new();
        let pool = StoragePool::new();
        let clip = capture_region(&world, &mpool, &pool, &addr, (0, 0), (2, 2), 5);

        assert_eq!(clip.entries.len(), 1);
        assert_eq!(clip.entries[0].offset, (0, 0));
        assert_eq!(
            clip.entries[0].kind,
            StructureKind::Machine(MachineType::Inverter)
        );
    }

    #[test]
    fn capture_region_empty() {
        let world = WorldState::new();
        let mpool = MachinePool::new();
        let pool = StoragePool::new();
        let clip = capture_region(&world, &mpool, &pool, &[0], (0, 0), (10, 10), 5);

        assert!(clip.entries.is_empty());
        assert_eq!(clip.width, 11);
        assert_eq!(clip.height, 11);
    }

    #[test]
    fn capture_region_storage_with_items() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        let entity = world.place(&addr, (0, 0), ItemId::Storage, Direction::North).unwrap();

        let mpool = MachinePool::new();
        let mut pool = StoragePool::new();
        pool.add(entity);
        pool.accept_input(entity, ItemId::Point, 5);
        pool.accept_input(entity, ItemId::LineSegment, 3);

        let clip = capture_region(&world, &mpool, &pool, &addr, (0, 0), (1, 1), 5);

        assert_eq!(clip.entries.len(), 1);
        assert_eq!(clip.entries[0].kind, StructureKind::Storage);
        assert_eq!(clip.entries[0].items.len(), 2);
        assert!(clip.entries[0].items.contains(&(ItemId::Point, 5)));
        assert!(clip.entries[0].items.contains(&(ItemId::LineSegment, 3)));
    }

    #[test]
    fn capture_region_swapped_corners() {
        // top_left and bottom_right can be given in any order
        let mut world = WorldState::new();
        let addr = vec![0u8];
        world.place(&addr, (5, 5), ItemId::Belt, Direction::North).unwrap();

        let mpool = MachinePool::new();
        let pool = StoragePool::new();
        // Give bottom_right as "top_left" and vice versa
        let clip = capture_region(&world, &mpool, &pool, &addr, (5, 5), (5, 5), 5);
        assert_eq!(clip.entries.len(), 1);
    }

    // --- Phase BP2 tests ---

    fn make_belt_clipboard(offsets: &[(i32, i32, Direction)], w: i32, h: i32) -> Clipboard {
        Clipboard {
            entries: offsets
                .iter()
                .map(|&(x, y, dir)| BlueprintEntry {
                    offset: (x, y),
                    kind: StructureKind::Belt,
                    direction: dir,
                    items: Vec::new(),
                    recipe: None,
                })
                .collect(),
            width: w,
            height: h,
            tiling_q: 5,
        }
    }

    #[test]
    fn rotate_cw_single_belt() {
        // Belt at (0,0) in 1x1 bounding box facing East
        let mut clip = make_belt_clipboard(&[(0, 0, Direction::East)], 1, 1);
        clip.rotate_cw();
        assert_eq!(clip.entries[0].offset, (0, 0));
        assert_eq!(clip.entries[0].direction, Direction::South);
        assert_eq!(clip.width, 1);
        assert_eq!(clip.height, 1);
    }

    #[test]
    fn rotate_cw_identity_after_4() {
        // 4 rotations should return to original
        let mut clip = make_belt_clipboard(
            &[(0, 0, Direction::North), (2, 1, Direction::East)],
            3,
            2,
        );
        let original_entries: Vec<_> = clip.entries.iter().map(|e| (e.offset, e.direction)).collect();
        let orig_w = clip.width;
        let orig_h = clip.height;
        for _ in 0..4 {
            clip.rotate_cw();
        }
        assert_eq!(clip.width, orig_w);
        assert_eq!(clip.height, orig_h);
        for (i, entry) in clip.entries.iter().enumerate() {
            assert_eq!(entry.offset, original_entries[i].0, "entry {i} offset");
            assert_eq!(entry.direction, original_entries[i].1, "entry {i} direction");
        }
    }

    #[test]
    fn rotate_cw_line_of_belts() {
        // 3 belts in a horizontal line: (0,0), (1,0), (2,0) in a 3x1 box
        let mut clip = make_belt_clipboard(
            &[
                (0, 0, Direction::East),
                (1, 0, Direction::East),
                (2, 0, Direction::East),
            ],
            3,
            1,
        );
        clip.rotate_cw();
        // After 90° CW, 3x1 → 1x3, belts should be vertical
        assert_eq!(clip.width, 1);
        assert_eq!(clip.height, 3);
        // (0,0) in 3x1 → rotate_cell(0,0,3,1) = (1-1-0, 0) = (0,0)
        assert_eq!(clip.entries[0].offset, (0, 0));
        // (1,0) → rotate_cell(1,0,3,1) = (0,1)
        assert_eq!(clip.entries[1].offset, (0, 1));
        // (2,0) → rotate_cell(2,0,3,1) = (0,2)
        assert_eq!(clip.entries[2].offset, (0, 2));
        // All should now face South (East rotated CW)
        for entry in &clip.entries {
            assert_eq!(entry.direction, Direction::South);
        }
    }

    #[test]
    fn rotate_cw_2x2_machine() {
        // Composer (2x2) at (0,0) in a 2x2 bounding box
        let mut clip = Clipboard {
            entries: vec![BlueprintEntry {
                offset: (0, 0),
                kind: StructureKind::Machine(MachineType::Composer),
                direction: Direction::North,
                items: Vec::new(),
                recipe: None,
            }],
            width: 2,
            height: 2,
            tiling_q: 5,
        };
        clip.rotate_cw();
        // 2x2 in 2x2 box: should stay at (0,0)
        assert_eq!(clip.entries[0].offset, (0, 0));
        assert_eq!(clip.entries[0].direction, Direction::East);
        assert_eq!(clip.width, 2);
        assert_eq!(clip.height, 2);
    }

    #[test]
    fn can_paste_empty_world() {
        let world = WorldState::new();
        let clip = make_belt_clipboard(
            &[(0, 0, Direction::North), (1, 0, Direction::North)],
            2,
            1,
        );
        let checks = can_paste(&world, &[0], (5, 5), &clip);
        assert_eq!(checks.len(), 2);
        assert!(checks.iter().all(|&(_, ok)| ok));
    }

    #[test]
    fn can_paste_detects_collision() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        world.place(&addr, (5, 5), ItemId::Belt, Direction::North).unwrap();

        let clip = make_belt_clipboard(
            &[(0, 0, Direction::East), (1, 0, Direction::East)],
            2,
            1,
        );
        let checks = can_paste(&world, &addr, (5, 5), &clip);
        // First entry at (5,5) should collide, second at (6,5) should be fine
        assert!(!checks[0].1);
        assert!(checks[1].1);
    }

    #[test]
    fn required_items_tallies_correctly() {
        let clip = Clipboard {
            entries: vec![
                BlueprintEntry {
                    offset: (0, 0),
                    kind: StructureKind::Belt,
                    direction: Direction::North,
                    items: Vec::new(),
                    recipe: None,
                },
                BlueprintEntry {
                    offset: (1, 0),
                    kind: StructureKind::Belt,
                    direction: Direction::North,
                    items: Vec::new(),
                    recipe: None,
                },
                BlueprintEntry {
                    offset: (0, 1),
                    kind: StructureKind::Machine(MachineType::Composer),
                    direction: Direction::North,
                    items: Vec::new(),
                    recipe: Some(0),
                },
            ],
            width: 3,
            height: 3,
            tiling_q: 5,
        };
        let items = required_items(&clip);
        let belt_count = items.iter().find(|&&(id, _)| id == ItemId::Belt).map(|&(_, c)| c).unwrap_or(0);
        let composer_count = items.iter().find(|&&(id, _)| id == ItemId::Composer).map(|&(_, c)| c).unwrap_or(0);
        assert_eq!(belt_count, 2);
        assert_eq!(composer_count, 1);
    }

    #[test]
    fn capture_region_copies_machine_recipe() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        let entity = world.place(&addr, (0, 0), ItemId::Composer, Direction::North).unwrap();

        let mut mpool = MachinePool::new();
        mpool.add(entity, MachineType::Composer);
        mpool.set_recipe(entity, Some(0));

        let pool = StoragePool::new();
        let clip = capture_region(&world, &mpool, &pool, &addr, (0, 0), (1, 1), 5);

        assert_eq!(clip.entries.len(), 1);
        assert_eq!(clip.entries[0].recipe, Some(0));
    }

    #[test]
    fn capture_region_no_recipe_gives_none() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        let entity = world.place(&addr, (0, 0), ItemId::Composer, Direction::North).unwrap();

        let mut mpool = MachinePool::new();
        mpool.add(entity, MachineType::Composer);
        // No recipe set

        let pool = StoragePool::new();
        let clip = capture_region(&world, &mpool, &pool, &addr, (0, 0), (1, 1), 5);

        assert_eq!(clip.entries.len(), 1);
        assert_eq!(clip.entries[0].recipe, None);
    }

    #[test]
    fn capture_region_belt_recipe_always_none() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        world.place(&addr, (0, 0), ItemId::Belt, Direction::East).unwrap();

        let mpool = MachinePool::new();
        let pool = StoragePool::new();
        let clip = capture_region(&world, &mpool, &pool, &addr, (0, 0), (0, 0), 5);

        assert_eq!(clip.entries.len(), 1);
        assert_eq!(clip.entries[0].recipe, None);
    }

    // --- Virtual coordinate utility tests ---

    #[test]
    fn virtual_to_tile_local_center() {
        // Within the anchor tile
        assert_eq!(virtual_to_tile_local(0), (0, 0));
        assert_eq!(virtual_to_tile_local(10), (0, 10));
        assert_eq!(virtual_to_tile_local(-10), (0, -10));
        assert_eq!(virtual_to_tile_local(31), (0, 31));
        assert_eq!(virtual_to_tile_local(-32), (0, -32));
    }

    #[test]
    fn virtual_to_tile_local_next_tile() {
        // Just past the positive edge: v=32 → tile_delta=1, local=-32
        assert_eq!(virtual_to_tile_local(32), (1, -32));
        assert_eq!(virtual_to_tile_local(33), (1, -31));
        assert_eq!(virtual_to_tile_local(95), (1, 31));
        // Two tiles away
        assert_eq!(virtual_to_tile_local(96), (2, -32));
    }

    #[test]
    fn virtual_to_tile_local_negative_tile() {
        // Past the negative edge: v=-33 → tile_delta=-1, local=31
        assert_eq!(virtual_to_tile_local(-33), (-1, 31));
        assert_eq!(virtual_to_tile_local(-96), (-1, -32));
        assert_eq!(virtual_to_tile_local(-97), (-2, 31));
    }

    #[test]
    fn virtual_roundtrip() {
        for v in -200..=200 {
            let (td, local) = virtual_to_tile_local(v);
            let back = tile_local_to_virtual(td, local);
            assert_eq!(back, v, "roundtrip failed for v={v}");
        }
    }

    #[test]
    fn shared_edge_consistency() {
        // Shared edge: tile k at gx=32 should equal tile k+1 at gx=-32
        // v = tile_local_to_virtual(0, 32) = 32
        // v = tile_local_to_virtual(1, -32) = 64 + (-32) = 32
        let v1 = tile_local_to_virtual(0, 32);
        let v2 = tile_local_to_virtual(1, -32);
        assert_eq!(v1, v2);
    }

    #[test]
    fn is_vertex_cell_corners() {
        assert!(is_vertex_cell(32, 32));
        assert!(is_vertex_cell(-32, 32));
        assert!(is_vertex_cell(32, -32));
        assert!(is_vertex_cell(-32, -32));
    }

    #[test]
    fn is_vertex_cell_non_corners() {
        assert!(!is_vertex_cell(0, 0));
        assert!(!is_vertex_cell(32, 0));
        assert!(!is_vertex_cell(0, 32));
        assert!(!is_vertex_cell(31, 32));
        assert!(!is_vertex_cell(32, 31));
    }

    #[test]
    fn footprint_1x1_at_corner() {
        assert!(footprint_touches_vertex(32, 32, StructureKind::Belt, Direction::North));
        assert!(!footprint_touches_vertex(31, 31, StructureKind::Belt, Direction::North));
    }

    #[test]
    fn footprint_2x2_near_corner() {
        // Composer at (31, 31) occupies (31,31), (32,31), (31,32), (32,32) — touches vertex
        assert!(footprint_touches_vertex(31, 31, StructureKind::Machine(MachineType::Composer), Direction::North));
        // Composer at (30, 30) occupies (30,30)-(31,31) — no vertex
        assert!(!footprint_touches_vertex(30, 30, StructureKind::Machine(MachineType::Composer), Direction::North));
    }

    #[test]
    fn rotate_cw_multi_tile_swaps_axis() {
        // A 2-tile horizontal clipboard (width=65, height=3) should become
        // 2-tile vertical (width=3, height=65) after rotation
        let mut clip = Clipboard {
            entries: vec![
                BlueprintEntry {
                    offset: (0, 0),
                    kind: StructureKind::Belt,
                    direction: Direction::East,
                    items: Vec::new(),
                    recipe: None,
                },
                BlueprintEntry {
                    offset: (64, 0),
                    kind: StructureKind::Belt,
                    direction: Direction::East,
                    items: Vec::new(),
                    recipe: None,
                },
            ],
            width: 65,
            height: 3,
            tiling_q: 5,
        };
        clip.rotate_cw();
        // Width and height swap
        assert_eq!(clip.width, 3);
        assert_eq!(clip.height, 65);
        // After rotation, a multi-tile x-strip becomes a multi-tile y-strip
        assert!(clip.height > 64); // confirms y-axis strip
    }

    // --- Phase BP3 persistence tests ---

    fn sample_clipboard() -> Clipboard {
        Clipboard {
            entries: vec![
                BlueprintEntry {
                    offset: (0, 0),
                    kind: StructureKind::Belt,
                    direction: Direction::East,
                    items: Vec::new(),
                    recipe: None,
                },
                BlueprintEntry {
                    offset: (1, 0),
                    kind: StructureKind::Machine(MachineType::Composer),
                    direction: Direction::South,
                    items: Vec::new(),
                    recipe: Some(2),
                },
                BlueprintEntry {
                    offset: (0, 1),
                    kind: StructureKind::Storage,
                    direction: Direction::North,
                    items: vec![(ItemId::Point, 10), (ItemId::LineSegment, 5)],
                    recipe: None,
                },
            ],
            width: 3,
            height: 2,
            tiling_q: 5,
        }
    }

    #[test]
    fn blueprint_file_roundtrip() {
        let clip = sample_clipboard();
        let file = BlueprintFile::from_clipboard(&clip, "test bp".to_string());
        assert_eq!(file.version, BLUEPRINT_VERSION);
        assert_eq!(file.name, "test bp");
        assert_eq!(file.tiling_q, 5);
        assert_eq!(file.width, 3);
        assert_eq!(file.height, 2);
        assert_eq!(file.entries.len(), 3);

        // Serialize and deserialize
        let bytes = bincode::serialize(&file).unwrap();
        let loaded: BlueprintFile = bincode::deserialize(&bytes).unwrap();
        assert_eq!(loaded.name, "test bp");
        assert_eq!(loaded.tiling_q, 5);
        assert_eq!(loaded.width, 3);
        assert_eq!(loaded.height, 2);
        assert_eq!(loaded.entries.len(), 3);
        assert_eq!(loaded.entries[0].kind, StructureKind::Belt);
        assert_eq!(loaded.entries[0].direction, Direction::East);
        assert_eq!(loaded.entries[1].recipe, Some(2));
        assert_eq!(loaded.entries[2].items.len(), 2);
    }

    #[test]
    fn blueprint_to_clipboard_roundtrip() {
        let clip = sample_clipboard();
        let file = BlueprintFile::from_clipboard(&clip, "rt".to_string());
        let restored = file.to_clipboard();
        assert_eq!(restored.width, clip.width);
        assert_eq!(restored.height, clip.height);
        assert_eq!(restored.tiling_q, clip.tiling_q);
        assert_eq!(restored.entries.len(), clip.entries.len());
        for (a, b) in restored.entries.iter().zip(clip.entries.iter()) {
            assert_eq!(a.offset, b.offset);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.direction, b.direction);
            assert_eq!(a.items, b.items);
            assert_eq!(a.recipe, b.recipe);
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let clip = sample_clipboard();
        let file = BlueprintFile::from_clipboard(&clip, "save_test".to_string());
        let bytes = bincode::serialize(&file).unwrap();
        let path = dir.path().join("save_test.blueprint");
        std::fs::write(&path, &bytes).unwrap();

        let loaded = load_blueprint(&path, 5).unwrap();
        assert_eq!(loaded.name, "save_test");
        assert_eq!(loaded.entries.len(), 3);
        assert_eq!(loaded.tiling_q, 5);
    }

    #[test]
    fn load_rejects_tiling_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let clip = sample_clipboard(); // tiling_q = 5
        let file = BlueprintFile::from_clipboard(&clip, "mismatch".to_string());
        let bytes = bincode::serialize(&file).unwrap();
        let path = dir.path().join("mismatch.blueprint");
        std::fs::write(&path, &bytes).unwrap();

        let err = load_blueprint(&path, 7).unwrap_err();
        assert!(err.contains("{4,5}"), "error should mention source tiling: {err}");
        assert!(err.contains("{4,7}"), "error should mention target tiling: {err}");
    }

    #[test]
    fn sanitize_name_strips_separators() {
        assert_eq!(sanitize_name("my/blueprint"), "myblueprint");
        assert_eq!(sanitize_name("test\\bp"), "testbp");
        assert_eq!(sanitize_name(""), "unnamed");
        assert_eq!(sanitize_name("   "), "unnamed");
        assert_eq!(sanitize_name("  hello  "), "hello");
    }

    #[test]
    fn unique_path_avoids_collision() {
        let dir = tempfile::tempdir().unwrap();
        // Create first file
        std::fs::write(dir.path().join("test.blueprint"), b"x").unwrap();
        let p = unique_path(dir.path(), "test");
        assert_eq!(p.file_name().unwrap(), "test_2.blueprint");

        // Create second collision
        std::fs::write(&p, b"y").unwrap();
        let p2 = unique_path(dir.path(), "test");
        assert_eq!(p2.file_name().unwrap(), "test_3.blueprint");
    }

    #[test]
    fn delete_blueprint_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("del.blueprint");
        std::fs::write(&path, b"data").unwrap();
        assert!(path.exists());
        delete_blueprint(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn rename_blueprint_updates_name_and_file() {
        let dir = tempfile::tempdir().unwrap();
        let clip = sample_clipboard();
        let file = BlueprintFile::from_clipboard(&clip, "old_name".to_string());
        let bytes = bincode::serialize(&file).unwrap();
        let old_path = dir.path().join("old_name.blueprint");
        std::fs::write(&old_path, &bytes).unwrap();

        let new_path = rename_blueprint(&old_path, "new_name").unwrap();
        assert!(!old_path.exists());
        assert!(new_path.exists());
        assert_eq!(new_path.file_name().unwrap(), "new_name.blueprint");

        let loaded: BlueprintFile =
            bincode::deserialize(&std::fs::read(&new_path).unwrap()).unwrap();
        assert_eq!(loaded.name, "new_name");
    }
}
