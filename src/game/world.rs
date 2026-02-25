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

    /// Integer grid-space unit offset.
    pub fn grid_offset_i32(self) -> (i32, i32) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }

    /// Number of 90° clockwise rotations from North to reach this direction.
    #[allow(dead_code)]
    pub fn rotations_from_north(self) -> u8 {
        match self {
            Self::North => 0,
            Self::East => 1,
            Self::South => 2,
            Self::West => 3,
        }
    }

    /// Rotate clockwise by `n` 90° steps.
    #[allow(dead_code)]
    pub fn rotate_n_cw(self, n: u8) -> Self {
        let mut d = self;
        for _ in 0..(n % 4) {
            d = d.rotate_cw();
        }
        d
    }

    /// Map this grid direction to the {4,n} tiling edge index (0–3).
    /// The neighbor transform at angle k·π/2 corresponds to: East=0, South=1, West=2, North=3.
    pub fn tiling_edge_index(self) -> u8 {
        match self {
            Self::East => 0,
            Self::South => 1,
            Self::West => 2,
            Self::North => 3,
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
    /// Footprint in grid cells: (width, height). Matches shader `machine_size()`.
    pub fn footprint(&self) -> (i32, i32) {
        match self {
            Self::Belt => (1, 1),
            Self::Machine(mt) => mt.footprint(),
            Self::PowerNode => (1, 1),   // Quadrupole
            Self::PowerSource => (2, 2), // Dynamo
        }
    }

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
            ItemId::SourceMachine => Some(Self::Machine(MachineType::Source)),
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

/// Compute all grid cells occupied by a structure with the given footprint placed at `origin`.
/// Footprint extends from origin in +x, +y direction.
pub fn occupied_cells(origin: (i32, i32), footprint: (i32, i32)) -> Vec<(i32, i32)> {
    let (w, h) = footprint;
    let mut cells = Vec::with_capacity((w * h) as usize);
    for dy in 0..h {
        for dx in 0..w {
            cells.push((origin.0 + dx, origin.1 + dy));
        }
    }
    cells
}

pub struct WorldState {
    /// Spatial index: tile address → (grid position → entity). "What's at this square?"
    tile_grid: HashMap<TileAddr, HashMap<(i32, i32), EntityId>>,
    /// Primary storage: entity → structure kind.
    structures: SlotMap<EntityId, StructureKind>,
    /// Entity → canonical world position (origin cell of multi-cell structures).
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

    /// Place a structure at the given tile address and grid position (origin cell).
    /// Multi-cell structures occupy all cells in their footprint extending from
    /// origin in +x, +y. Returns the entity ID on success, or `None` if any cell
    /// in the footprint is occupied or the item isn't a placeable structure.
    pub fn place(
        &mut self,
        address: &[u8],
        grid_xy: (i32, i32),
        item: ItemId,
        direction: Direction,
    ) -> Option<EntityId> {
        let kind = StructureKind::from_item(item)?;
        let footprint = kind.footprint();
        let cells = occupied_cells(grid_xy, footprint);
        let tile_addr = TileAddr::from_slice(address);
        let tile_slots = self.tile_grid.entry(tile_addr.clone()).or_default();

        // Check all cells in footprint are free
        for &cell in &cells {
            if tile_slots.contains_key(&cell) {
                return None;
            }
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

        // Register all occupied cells
        for &cell in &cells {
            tile_slots.insert(cell, entity);
        }
        Some(entity)
    }

    /// Check if `(gx, gy)` is the origin cell of the given entity.
    pub fn is_origin(&self, entity: EntityId, gx: i32, gy: i32) -> bool {
        self.positions
            .get(entity)
            .map(|pos| pos.gx as i32 == gx && pos.gy as i32 == gy)
            .unwrap_or(false)
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
        let &entity = tile_slots.get(&grid_xy)?;
        let kind = self.structures.get(entity)?;
        let footprint = kind.footprint();

        // Find origin for this entity
        let origin = self.positions.get(entity)?;
        let origin_xy = (origin.gx as i32, origin.gy as i32);
        let cells = occupied_cells(origin_xy, footprint);

        // Remove all occupied cells
        for &cell in &cells {
            tile_slots.remove(&cell);
        }

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
        assert!(world.place(&addr, (5, 10), ItemId::Belt, Direction::North).is_some());

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
        assert!(world.place(&addr, (0, 0), ItemId::Belt, Direction::North).is_some());
        assert!(world.place(&addr, (0, 0), ItemId::Quadrupole, Direction::East).is_none());
    }

    #[test]
    fn test_multi_cell_placement() {
        let mut world = WorldState::new();
        let addr = vec![0];
        // Composer is 2x2: occupies (5,5), (6,5), (5,6), (6,6)
        let entity = world.place(&addr, (5, 5), ItemId::Composer, Direction::North).unwrap();

        // All 4 cells should map to the same entity
        let entities = world.tile_entities(&addr).unwrap();
        assert_eq!(entities.get(&(5, 5)), Some(&entity));
        assert_eq!(entities.get(&(6, 5)), Some(&entity));
        assert_eq!(entities.get(&(5, 6)), Some(&entity));
        assert_eq!(entities.get(&(6, 6)), Some(&entity));

        // Origin is (5, 5)
        assert!(world.is_origin(entity, 5, 5));
        assert!(!world.is_origin(entity, 6, 5));
    }

    #[test]
    fn test_multi_cell_overlap_blocked() {
        let mut world = WorldState::new();
        let addr = vec![0];
        // Composer at (5,5) occupies (5,5)-(6,6)
        world.place(&addr, (5, 5), ItemId::Composer, Direction::North).unwrap();
        // Another Composer at (6,6) would overlap at (6,6)
        assert!(world.place(&addr, (6, 6), ItemId::Composer, Direction::North).is_none());
        // Belt at (6,5) overlaps with the Composer
        assert!(world.place(&addr, (6, 5), ItemId::Belt, Direction::North).is_none());
        // Belt at (7,5) is free
        assert!(world.place(&addr, (7, 5), ItemId::Belt, Direction::North).is_some());
    }

    #[test]
    fn test_multi_cell_click_any_cell() {
        let mut world = WorldState::new();
        let addr = vec![0];
        let entity = world.place(&addr, (10, 10), ItemId::Composer, Direction::North).unwrap();

        // Clicking any cell of the 2x2 machine returns the same entity
        let entities = world.tile_entities(&addr).unwrap();
        for dy in 0..2 {
            for dx in 0..2 {
                let &found = entities.get(&(10 + dx, 10 + dy)).unwrap();
                assert_eq!(found, entity);
                assert_eq!(world.kind(found), Some(StructureKind::Machine(MachineType::Composer)));
            }
        }
    }

    #[test]
    fn test_multi_cell_remove() {
        let mut world = WorldState::new();
        let addr = vec![0];
        world.place(&addr, (5, 5), ItemId::Composer, Direction::North).unwrap();
        // Remove via any cell in the footprint
        let removed = world.remove(&addr, (6, 6));
        assert_eq!(removed, Some(ItemId::Composer));
        // All cells should be free now
        assert!(world.tile_entities(&addr).is_none());
    }

    #[test]
    fn test_3x2_footprint() {
        let mut world = WorldState::new();
        let addr = vec![0];
        // Inverter is 3x2: occupies (0,0), (1,0), (2,0), (0,1), (1,1), (2,1)
        let entity = world.place(&addr, (0, 0), ItemId::Inverter, Direction::North).unwrap();
        let entities = world.tile_entities(&addr).unwrap();
        assert_eq!(entities.len(), 6);
        for dy in 0..2 {
            for dx in 0..3 {
                assert_eq!(entities.get(&(dx, dy)), Some(&entity));
            }
        }
    }

    #[test]
    fn test_remove() {
        let mut world = WorldState::new();
        let addr = vec![2];
        world.place(&addr, (1, 1), ItemId::Belt, Direction::South).unwrap();
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
        world.place(&addr, (32, -16), ItemId::Belt, Direction::East).unwrap();

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
        let e1 = world.place(&addr, (0, 0), ItemId::Belt, Direction::North).unwrap();

        // Place more entities
        world.place(&addr, (1, 0), ItemId::Belt, Direction::East).unwrap();
        world.place(&addr, (2, 0), ItemId::Quadrupole, Direction::South).unwrap();

        // Original entity still valid
        assert_eq!(world.kind(e1), Some(StructureKind::Belt));
        assert_eq!(world.direction(e1), Some(Direction::North));
    }
}
