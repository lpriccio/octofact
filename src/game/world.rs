use std::collections::HashMap;
use slotmap::{new_key_type, SecondaryMap, SlotMap};
use super::items::{ItemId, MachineType};
use crate::hyperbolic::tiling::TileAddr;

new_key_type! {
    /// Stable handle into the world's entity storage. Generational index
    /// via SlotMap — safe to hold across insertions and removals.
    pub struct EntityId;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub fn rotate_cw(self) -> Self {
        match self {
            Self::North => Self::East,
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
        }
    }

    pub fn arrow_char(self) -> char {
        match self {
            Self::North => '\u{2191}', // ↑
            Self::East => '\u{2192}',  // →
            Self::South => '\u{2193}', // ↓
            Self::West => '\u{2190}',  // ←
        }
    }

    /// Grid-space unit offset: (dx, dy) where +x = East, +y = South in grid coords.
    pub fn grid_offset(self) -> (f64, f64) {
        match self {
            Self::North => (0.0, -1.0),
            Self::East => (1.0, 0.0),
            Self::South => (0.0, 1.0),
            Self::West => (-1.0, 0.0),
        }
    }
}

/// Functional type of a placed structure. Determines which simulation
/// system processes it. Simulation pool IDs (BeltId, MachineId, etc.)
/// will be added in later phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureKind {
    Belt,
    Machine(MachineType),
    PowerNode,   // Quadrupole
    PowerSource, // Dynamo
}

impl StructureKind {
    /// Derive structure kind from the item being placed.
    /// Returns `None` for non-placeable items (raw resources, intermediates).
    pub fn from_item(item: ItemId) -> Option<Self> {
        match item {
            ItemId::Belt => Some(Self::Belt),
            ItemId::Quadrupole => Some(Self::PowerNode),
            ItemId::Dynamo => Some(Self::PowerSource),
            ItemId::Composer => Some(Self::Machine(MachineType::Composer)),
            ItemId::Inverter => Some(Self::Machine(MachineType::Inverter)),
            ItemId::Embedder => Some(Self::Machine(MachineType::Embedder)),
            ItemId::Quotient => Some(Self::Machine(MachineType::Quotient)),
            ItemId::Transformer => Some(Self::Machine(MachineType::Transformer)),
            _ => None,
        }
    }
}

/// Canonical position of a placed entity: tile address + grid coordinates.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GridPos {
    pub tile: TileAddr,
    pub gx: i16,
    pub gy: i16,
}

pub struct WorldState {
    /// Spatial index: tile address → (grid position → entity). "What's at this square?"
    tile_grid: HashMap<TileAddr, HashMap<(i32, i32), EntityId>>,
    /// Primary storage: entity → structure kind.
    structures: SlotMap<EntityId, StructureKind>,
    /// Entity → canonical world position.
    positions: SecondaryMap<EntityId, GridPos>,
    /// Entity → facing direction.
    directions: SecondaryMap<EntityId, Direction>,
    /// Entity → source item (for inventory return on removal, display).
    items: SecondaryMap<EntityId, ItemId>,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            tile_grid: HashMap::new(),
            structures: SlotMap::with_key(),
            positions: SecondaryMap::new(),
            directions: SecondaryMap::new(),
            items: SecondaryMap::new(),
        }
    }

    /// Place a structure at the given tile address and grid position.
    /// Returns `true` if placement succeeded, `false` if the cell is
    /// occupied or the item isn't a placeable structure.
    pub fn place(
        &mut self,
        address: &[u8],
        grid_xy: (i32, i32),
        item: ItemId,
        direction: Direction,
    ) -> bool {
        let kind = match StructureKind::from_item(item) {
            Some(k) => k,
            None => return false,
        };
        let tile_addr = TileAddr::from_slice(address);
        let tile_slots = self.tile_grid.entry(tile_addr.clone()).or_default();
        if tile_slots.contains_key(&grid_xy) {
            return false;
        }
        let entity = self.structures.insert(kind);
        self.positions.insert(
            entity,
            GridPos {
                tile: tile_addr,
                gx: grid_xy.0 as i16,
                gy: grid_xy.1 as i16,
            },
        );
        self.directions.insert(entity, direction);
        self.items.insert(entity, item);
        tile_slots.insert(grid_xy, entity);
        true
    }

    /// All entity positions within a tile. Returns the grid→entity map.
    pub fn tile_entities(&self, address: &[u8]) -> Option<&HashMap<(i32, i32), EntityId>> {
        self.tile_grid.get(address)
    }

    /// Look up an entity's facing direction.
    pub fn direction(&self, entity: EntityId) -> Option<Direction> {
        self.directions.get(entity).copied()
    }

    /// Look up an entity's structure kind.
    #[allow(dead_code)]
    pub fn kind(&self, entity: EntityId) -> Option<StructureKind> {
        self.structures.get(entity).copied()
    }

    /// Look up the item an entity was placed from.
    #[allow(dead_code)]
    pub fn item(&self, entity: EntityId) -> Option<ItemId> {
        self.items.get(entity).copied()
    }

    /// Look up an entity's canonical position.
    #[allow(dead_code)]
    pub fn position(&self, entity: EntityId) -> Option<&GridPos> {
        self.positions.get(entity)
    }
}

#[cfg(test)]
impl WorldState {
    pub fn remove(&mut self, address: &[u8], grid_xy: (i32, i32)) -> Option<ItemId> {
        let tile_slots = self.tile_grid.get_mut(address)?;
        let entity = tile_slots.remove(&grid_xy)?;
        let item = self.items.remove(entity);
        self.structures.remove(entity);
        self.positions.remove(entity);
        self.directions.remove(entity);
        if tile_slots.is_empty() {
            self.tile_grid.remove(address);
        }
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_rotate_cw() {
        assert_eq!(Direction::North.rotate_cw(), Direction::East);
        assert_eq!(Direction::East.rotate_cw(), Direction::South);
        assert_eq!(Direction::South.rotate_cw(), Direction::West);
        assert_eq!(Direction::West.rotate_cw(), Direction::North);
    }

    #[test]
    fn test_direction_arrow_char() {
        assert_eq!(Direction::North.arrow_char(), '\u{2191}');
        assert_eq!(Direction::East.arrow_char(), '\u{2192}');
        assert_eq!(Direction::South.arrow_char(), '\u{2193}');
        assert_eq!(Direction::West.arrow_char(), '\u{2190}');
    }

    #[test]
    fn test_place_and_query() {
        let mut world = WorldState::new();
        let addr = vec![0, 1];
        assert!(world.place(&addr, (5, 10), ItemId::Belt, Direction::North));

        let entities = world.tile_entities(&addr).unwrap();
        assert_eq!(entities.len(), 1);
        let &entity = entities.get(&(5, 10)).unwrap();
        assert_eq!(world.kind(entity), Some(StructureKind::Belt));
        assert_eq!(world.direction(entity), Some(Direction::North));
        assert_eq!(world.item(entity), Some(ItemId::Belt));
    }

    #[test]
    fn test_place_occupied() {
        let mut world = WorldState::new();
        let addr = vec![0];
        assert!(world.place(&addr, (0, 0), ItemId::Belt, Direction::North));
        assert!(!world.place(&addr, (0, 0), ItemId::Quadrupole, Direction::East));
    }

    #[test]
    fn test_remove() {
        let mut world = WorldState::new();
        let addr = vec![2];
        world.place(&addr, (1, 1), ItemId::Belt, Direction::South);
        let removed = world.remove(&addr, (1, 1));
        assert_eq!(removed, Some(ItemId::Belt));
        // Tile grid should be cleaned up
        assert!(world.tile_entities(&addr).is_none());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut world = WorldState::new();
        assert!(world.remove(&[0], (0, 0)).is_none());
    }

    #[test]
    fn test_structure_kind_from_item() {
        assert_eq!(StructureKind::from_item(ItemId::Belt), Some(StructureKind::Belt));
        assert_eq!(StructureKind::from_item(ItemId::Quadrupole), Some(StructureKind::PowerNode));
        assert_eq!(StructureKind::from_item(ItemId::Dynamo), Some(StructureKind::PowerSource));
        assert_eq!(
            StructureKind::from_item(ItemId::Composer),
            Some(StructureKind::Machine(MachineType::Composer))
        );
        // Raw resources aren't placeable
        assert_eq!(StructureKind::from_item(ItemId::NullSet), None);
        assert_eq!(StructureKind::from_item(ItemId::Point), None);
    }

    #[test]
    fn test_grid_pos_stored() {
        let mut world = WorldState::new();
        let addr = vec![3, 1, 4];
        world.place(&addr, (32, -16), ItemId::Belt, Direction::East);

        let entities = world.tile_entities(&addr).unwrap();
        let &entity = entities.get(&(32, -16)).unwrap();
        let pos = world.position(entity).unwrap();
        assert_eq!(pos.gx, 32);
        assert_eq!(pos.gy, -16);
        assert_eq!(pos.tile.as_slice(), &[3, 1, 4]);
    }

    #[test]
    fn test_entity_id_stable_after_other_inserts() {
        let mut world = WorldState::new();
        let addr = vec![0];
        world.place(&addr, (0, 0), ItemId::Belt, Direction::North);
        let &e1 = world.tile_entities(&addr).unwrap().get(&(0, 0)).unwrap();

        // Place more entities
        world.place(&addr, (1, 0), ItemId::Belt, Direction::East);
        world.place(&addr, (2, 0), ItemId::Quadrupole, Direction::South);

        // Original entity still valid
        assert_eq!(world.kind(e1), Some(StructureKind::Belt));
        assert_eq!(world.direction(e1), Some(Direction::North));
    }
}
