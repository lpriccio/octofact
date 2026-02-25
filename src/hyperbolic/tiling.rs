use std::collections::{HashMap, HashSet, VecDeque};

use smallvec::SmallVec;

use super::poincare::{Complex, Mobius, TilingConfig, center_to_center_distance, neighbor_transforms, poincare_distance};

/// Tile address: sequence of direction indices from origin.
/// 12 bytes inline covers depth 12 (~16M tiles) without heap allocation.
pub type TileAddr = SmallVec<[u8; 12]>;

/// A tile in the {p,q} tiling, identified by its canonical address.
#[derive(Clone, Debug)]
pub struct Tile {
    /// Canonical address: sequence of direction indices (0..p-1) from origin.
    /// Empty = origin tile.
    pub address: TileAddr,
    /// Mobius transform mapping the canonical polygon to this tile's position.
    pub transform: Mobius,
    /// BFS depth (= address length).
    pub depth: usize,
    /// Parity: false = even (same orientation as origin), true = odd (flipped).
    /// For even-p tilings this is unused but tracked for consistency.
    pub parity: bool,
}

/// Spatial dedup key: discretize Poincare disk position to grid.
/// Precision 1e3 (grid cell ~0.001) tolerates floating-point drift from
/// repeated Mobius compositions during rebase, while still distinguishing
/// adjacent tile centers (min ~0.03 apart at the visibility boundary).
pub(crate) fn spatial_key(z: Complex) -> (i64, i64) {
    ((z.re * 1e3).round() as i64, (z.im * 1e3).round() as i64)
}

/// BFS tiling state for incremental expansion of a {p,q} tiling.
pub struct TilingState {
    pub cfg: TilingConfig,
    pub tiles: Vec<Tile>,
    seen: HashSet<(i64, i64)>,
    spatial_to_tile: HashMap<(i64, i64), usize>,
    frontier: VecDeque<usize>,
    /// `[0]` = transforms for even-parity tiles, `[1]` = for odd-parity tiles.
    pub neighbor_xforms: [Vec<Mobius>; 2],
}

impl TilingState {
    pub fn new(cfg: TilingConfig) -> Self {
        let origin = Tile {
            address: TileAddr::new(),
            transform: Mobius::identity(),
            depth: 0,
            parity: false,
        };
        let key = spatial_key(Complex::ZERO);
        let mut seen = HashSet::new();
        seen.insert(key);
        let mut spatial_to_tile = HashMap::new();
        spatial_to_tile.insert(key, 0);

        let mut frontier = VecDeque::new();
        frontier.push_back(0);

        Self {
            cfg,
            tiles: vec![origin],
            seen,
            spatial_to_tile,
            frontier,
            neighbor_xforms: neighbor_transforms(&cfg),
        }
    }

    /// Expand only frontier tiles within `max_dist` hyperbolic distance of `target`.
    /// Frontier tiles that are too far are deferred (kept in frontier for later).
    pub fn expand_near(&mut self, target: Complex, max_dist: f64) {
        let frontier_len = self.frontier.len();
        if frontier_len == 0 {
            return;
        }
        let mut deferred = VecDeque::new();
        for _ in 0..frontier_len {
            let parent_idx = self.frontier.pop_front().unwrap();
            let parent_center = self.tiles[parent_idx].transform.apply(Complex::ZERO);
            if poincare_distance(parent_center, target) > max_dist {
                deferred.push_back(parent_idx);
                continue;
            }
            let parent_transform = self.tiles[parent_idx].transform;
            let parent_address = self.tiles[parent_idx].address.clone();
            let parent_depth = self.tiles[parent_idx].depth;
            let parent_parity = self.tiles[parent_idx].parity;
            let xforms = &self.neighbor_xforms[parent_parity as usize];
            for dir in 0..self.cfg.p as u8 {
                let child_transform =
                    parent_transform.compose(&xforms[dir as usize]);
                let center = child_transform.apply(Complex::ZERO);
                let key = spatial_key(center);
                if self.seen.contains(&key) {
                    continue;
                }
                self.seen.insert(key);
                let mut child_address = parent_address.clone();
                child_address.push(dir);
                let child = Tile {
                    address: child_address,
                    transform: child_transform,
                    depth: parent_depth + 1,
                    parity: !parent_parity,
                };
                let child_idx = self.tiles.len();
                self.spatial_to_tile.insert(key, child_idx);
                self.tiles.push(child);
                self.frontier.push_back(child_idx);
            }
        }
        // Return deferred tiles to the front of the frontier
        while let Some(idx) = deferred.pop_back() {
            self.frontier.push_front(idx);
        }
    }

    /// Expand BFS until every frontier tile is at least `min_layers` hops
    /// (in center-to-center distance) away from `target`.
    /// Since this grows the existing BFS from origin, addresses stay canonical.
    pub fn ensure_coverage(&mut self, target: Complex, min_layers: usize) {
        let d = center_to_center_distance(&self.cfg);
        // We need the frontier pushed out beyond min_layers * D from target.
        // Add 0.5 buffer to ensure full coverage of the outermost ring.
        let required_dist = (min_layers as f64 + 0.5) * d;

        for _ in 0..20 {
            if self.frontier.is_empty() {
                break;
            }
            // Check if any frontier tile is closer than required_dist to target
            let needs_more = self.frontier.iter().any(|&idx| {
                let c = self.tiles[idx].transform.apply(Complex::ZERO);
                poincare_distance(c, target) < required_dist
            });
            if !needs_more {
                break;
            }
            self.expand_near(target, required_dist);
        }
    }

    /// Find the tile index whose center is nearest to `pos` (O(1) spatial lookup).
    pub fn find_tile_near(&self, pos: Complex) -> Option<usize> {
        let key = spatial_key(pos);
        self.spatial_to_tile.get(&key).copied()
    }

    /// Find the address of the tile adjacent to `tile_idx` across edge `edge` (0..p-1).
    /// Returns None if the neighbor tile hasn't been expanded yet.
    pub fn neighbor_tile_addr(&self, tile_idx: usize, edge: u8) -> Option<TileAddr> {
        let tile = &self.tiles[tile_idx];
        let xforms = &self.neighbor_xforms[tile.parity as usize];
        let neighbor_transform = tile.transform.compose(&xforms[edge as usize]);
        let center = neighbor_transform.apply(Complex::ZERO);
        let key = spatial_key(center);
        let &idx = self.spatial_to_tile.get(&key)?;
        Some(self.tiles[idx].address.clone())
    }

    /// Recenter the tiling so that `center_idx` becomes the origin.
    /// Recomputes ALL tile transforms fresh from their canonical addresses,
    /// eliminating accumulated floating-point drift from repeated compositions.
    pub fn recenter_on(&mut self, center_idx: usize) {
        let center_abs = compute_transform_from_address(
            &self.tiles[center_idx].address,
            &self.neighbor_xforms,
        );
        let inv_center = center_abs.inverse();
        for tile in &mut self.tiles {
            let tile_abs =
                compute_transform_from_address(&tile.address, &self.neighbor_xforms);
            tile.transform = inv_center.compose(&tile_abs);
        }

        // Rebuild seen set and spatial index from all tiles
        self.seen.clear();
        self.spatial_to_tile.clear();
        for (idx, tile) in self.tiles.iter().enumerate() {
            let center = tile.transform.apply(Complex::ZERO);
            let key = spatial_key(center);
            self.seen.insert(key);
            self.spatial_to_tile.insert(key, idx);
        }

        // Rebuild frontier: tiles with any missing neighbor
        self.frontier.clear();
        for (idx, tile) in self.tiles.iter().enumerate() {
            let parity = tile.parity;
            let xforms = &self.neighbor_xforms[parity as usize];
            let is_boundary = (0..self.cfg.p as u8).any(|dir| {
                let neighbor = tile.transform.compose(&xforms[dir as usize]);
                let center = neighbor.apply(Complex::ZERO);
                !self.seen.contains(&spatial_key(center))
            });
            if is_boundary {
                self.frontier.push_back(idx);
            }
        }
    }
}

/// Recompute a tile's absolute Mobius transform from its canonical address.
/// Each step composes the corresponding neighbor transform, alternating parity.
fn compute_transform_from_address(address: &[u8], neighbor_xforms: &[Vec<Mobius>; 2]) -> Mobius {
    let mut t = Mobius::identity();
    let mut parity = 0usize; // origin is even
    for &dir in address {
        t = t.compose(&neighbor_xforms[parity][dir as usize]);
        parity ^= 1;
    }
    t
}

/// Format a tile address for display.
pub fn format_address(addr: &[u8]) -> String {
    if addr.is_empty() {
        "O".to_string()
    } else if addr.len() > 5 {
        let tail: String = addr[addr.len() - 5..].iter().map(|d| d.to_string()).collect();
        format!("..{tail}")
    } else {
        addr.iter().map(|d| d.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg83() -> TilingConfig {
        TilingConfig::new(8, 3)
    }

    #[test]
    fn test_origin_tile() {
        let state = TilingState::new(cfg83());
        assert_eq!(state.tiles.len(), 1);
        assert!(state.tiles[0].address.is_empty());
        assert_eq!(state.tiles[0].depth, 0);
    }

    #[test]
    fn test_ensure_coverage_depth_1() {
        let mut state = TilingState::new(cfg83());
        state.ensure_coverage(Complex::ZERO, 1);
        assert!(state.tiles.len() > 1, "should have more than just the origin");
    }

    #[test]
    fn test_ensure_coverage_depth_3() {
        let mut state = TilingState::new(cfg83());
        state.ensure_coverage(Complex::ZERO, 3);
        let count = state.tiles.len();
        assert!(count > 20, "too few tiles: {count}");
        assert!(count < 2000, "too many tiles: {count}");
    }

    #[test]
    fn test_all_addresses_unique() {
        let mut state = TilingState::new(cfg83());
        state.ensure_coverage(Complex::ZERO, 2);
        let mut addrs: HashSet<TileAddr> = HashSet::new();
        for tile in &state.tiles {
            assert!(
                addrs.insert(tile.address.clone()),
                "duplicate address: {:?}",
                tile.address
            );
        }
    }

    #[test]
    fn test_all_centers_inside_disk() {
        let mut state = TilingState::new(cfg83());
        state.ensure_coverage(Complex::ZERO, 3);
        for tile in &state.tiles {
            let c = tile.transform.apply(Complex::ZERO);
            assert!(
                c.abs() < 1.0,
                "tile {:?} center outside disk: {}",
                tile.address,
                c.abs()
            );
        }
    }

    #[test]
    fn test_73_centers_inside_disk() {
        let mut state = TilingState::new(TilingConfig::new(7, 3));
        state.ensure_coverage(Complex::ZERO, 3);
        for tile in &state.tiles {
            let c = tile.transform.apply(Complex::ZERO);
            assert!(
                c.abs() < 1.0,
                "tile {:?} center outside disk: {}",
                tile.address,
                c.abs()
            );
        }
    }

    #[test]
    fn test_format_address() {
        assert_eq!(format_address(&[]), "O");
        assert_eq!(format_address(&[0]), "0");
        assert_eq!(format_address(&[0, 0, 0, 0, 2]), "00002");
        assert_eq!(format_address(&[7, 3, 1]), "731");
        assert_eq!(format_address(&[1, 2, 3, 4, 5, 6, 7, 0]), "..45670");
    }
}
