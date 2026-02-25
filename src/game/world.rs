use std::collections::HashMap;
use super::items::ItemId;
use crate::hyperbolic::tiling::TileAddr;

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

#[derive(Clone, Debug)]
pub struct Structure {
    pub item: ItemId,
    pub direction: Direction,
}

#[derive(Clone, Debug, Default)]
pub struct CellState {
    pub structures: HashMap<(i32, i32), Structure>,
}

pub struct WorldState {
    cells: HashMap<TileAddr, CellState>,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            cells: HashMap::new(),
        }
    }

    pub fn place(&mut self, address: &[u8], grid_xy: (i32, i32), structure: Structure) -> bool {
        let cell = self.cells.entry(TileAddr::from_slice(address)).or_default();
        if cell.structures.contains_key(&grid_xy) {
            return false;
        }
        cell.structures.insert(grid_xy, structure);
        true
    }

    pub fn remove(&mut self, address: &[u8], grid_xy: (i32, i32)) -> Option<Structure> {
        let cell = self.cells.get_mut(address)?;
        let s = cell.structures.remove(&grid_xy);
        if cell.structures.is_empty() {
            self.cells.remove(address);
        }
        s
    }

    pub fn get_cell(&self, address: &[u8]) -> Option<&CellState> {
        self.cells.get(address)
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
    fn test_place_and_get() {
        let mut world = WorldState::new();
        let addr = vec![0, 1];
        let s = Structure { item: ItemId::Belt, direction: Direction::North };
        assert!(world.place(&addr, (5, 10), s));
        let cell = world.get_cell(&addr).unwrap();
        assert_eq!(cell.structures.len(), 1);
        assert_eq!(cell.structures[&(5, 10)].item, ItemId::Belt);
    }

    #[test]
    fn test_place_occupied() {
        let mut world = WorldState::new();
        let addr = vec![0];
        let s1 = Structure { item: ItemId::Belt, direction: Direction::North };
        let s2 = Structure { item: ItemId::Quadrupole, direction: Direction::East };
        assert!(world.place(&addr, (0, 0), s1));
        assert!(!world.place(&addr, (0, 0), s2));
    }

    #[test]
    fn test_remove() {
        let mut world = WorldState::new();
        let addr = vec![2];
        let s = Structure { item: ItemId::Belt, direction: Direction::South };
        world.place(&addr, (1, 1), s);
        let removed = world.remove(&addr, (1, 1));
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().item, ItemId::Belt);
        // Cell should be cleaned up
        assert!(world.get_cell(&addr).is_none());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut world = WorldState::new();
        assert!(world.remove(&[0], (0, 0)).is_none());
    }
}
