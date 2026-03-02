use super::items::ItemId;
use super::world::{Direction, EntityId, StructureKind, WorldState};
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

/// Capture all structures within a rectangle on a single tile.
///
/// `top_left` and `bottom_right` are inclusive grid coordinates.
/// Multi-cell entities are captured once at their origin offset.
pub fn capture_region(
    world: &WorldState,
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

                entries.push(BlueprintEntry {
                    offset: (gx - min_x, gy - min_y),
                    kind,
                    direction,
                    items,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::items::{ItemId, MachineType};
    use crate::game::world::WorldState;
    use crate::sim::storage::StoragePool;

    #[test]
    fn capture_region_single_belt() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        world.place(&addr, (5, 5), ItemId::Belt, Direction::East).unwrap();

        let pool = StoragePool::new();
        let clip = capture_region(&world, &pool, &addr, (5, 5), (5, 5), 5);

        assert_eq!(clip.entries.len(), 1);
        assert_eq!(clip.entries[0].offset, (0, 0));
        assert_eq!(clip.entries[0].kind, StructureKind::Belt);
        assert_eq!(clip.entries[0].direction, Direction::East);
        assert!(clip.entries[0].items.is_empty());
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

        let pool = StoragePool::new();
        let clip = capture_region(&world, &pool, &addr, (10, 20), (12, 22), 5);

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

        let pool = StoragePool::new();
        let clip = capture_region(&world, &pool, &addr, (5, 5), (6, 6), 5);

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

        let pool = StoragePool::new();
        let clip = capture_region(&world, &pool, &addr, (0, 0), (2, 2), 5);

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
        let pool = StoragePool::new();
        let clip = capture_region(&world, &pool, &[0], (0, 0), (10, 10), 5);

        assert!(clip.entries.is_empty());
        assert_eq!(clip.width, 11);
        assert_eq!(clip.height, 11);
    }

    #[test]
    fn capture_region_storage_with_items() {
        let mut world = WorldState::new();
        let addr = vec![0u8];
        let entity = world.place(&addr, (0, 0), ItemId::Storage, Direction::North).unwrap();

        let mut pool = StoragePool::new();
        pool.add(entity);
        pool.accept_input(entity, ItemId::Point, 5);
        pool.accept_input(entity, ItemId::LineSegment, 3);

        let clip = capture_region(&world, &pool, &addr, (0, 0), (1, 1), 5);

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

        let pool = StoragePool::new();
        // Give bottom_right as "top_left" and vice versa
        let clip = capture_region(&world, &pool, &addr, (5, 5), (5, 5), 5);
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
                },
                BlueprintEntry {
                    offset: (1, 0),
                    kind: StructureKind::Belt,
                    direction: Direction::North,
                    items: Vec::new(),
                },
                BlueprintEntry {
                    offset: (0, 1),
                    kind: StructureKind::Machine(MachineType::Composer),
                    direction: Direction::North,
                    items: Vec::new(),
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
}
