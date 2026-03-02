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
#[allow(dead_code)] // width/height/tiling_q read by Phase BP2 (paste mode)
pub struct Clipboard {
    pub entries: Vec<BlueprintEntry>,
    pub width: i32,
    pub height: i32,
    /// The tiling q parameter when this was captured.
    pub tiling_q: u32,
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
}
