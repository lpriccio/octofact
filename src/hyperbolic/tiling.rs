use std::collections::{HashSet, VecDeque};

use super::poincare::{Complex, Mobius, center_to_center_distance, neighbor_transforms, poincare_distance};

/// A tile in the {8,3} tiling, identified by its canonical address.
#[derive(Clone, Debug)]
pub struct Tile {
    /// Canonical address: sequence of direction indices (0-7) from origin.
    /// Empty = origin tile.
    pub address: Vec<u8>,
    /// Mobius transform mapping the canonical octagon to this tile's position.
    pub transform: Mobius,
    /// BFS depth (= address length).
    pub depth: usize,
    /// Extra elevation accumulated from clicks.
    pub extra_elevation: f32,
}

/// Spatial dedup key: discretize Poincare disk position to grid.
/// Precision 1e3 (grid cell ~0.001) tolerates floating-point drift from
/// repeated Mobius compositions during rebase, while still distinguishing
/// adjacent tile centers (min ~0.03 apart at the visibility boundary).
fn spatial_key(z: Complex) -> (i64, i64) {
    ((z.re * 1e3).round() as i64, (z.im * 1e3).round() as i64)
}

/// BFS tiling state for incremental expansion of the {8,3} tiling.
pub struct TilingState {
    pub tiles: Vec<Tile>,
    seen: HashSet<(i64, i64)>,
    frontier: VecDeque<usize>,
    pub neighbor_xforms: [Mobius; 8],
}

impl TilingState {
    pub fn new() -> Self {
        let origin = Tile {
            address: vec![],
            transform: Mobius::identity(),
            depth: 0,
            extra_elevation: 0.0,
        };
        let key = spatial_key(Complex::ZERO);
        let mut seen = HashSet::new();
        seen.insert(key);

        let mut frontier = VecDeque::new();
        frontier.push_back(0);

        Self {
            tiles: vec![origin],
            seen,
            frontier,
            neighbor_xforms: neighbor_transforms(),
        }
    }

    /// Expand the tiling by `steps` BFS layers.
    pub fn expand(&mut self, steps: usize) {
        for _ in 0..steps {
            let frontier_len = self.frontier.len();
            if frontier_len == 0 {
                break;
            }
            for _ in 0..frontier_len {
                let parent_idx = self.frontier.pop_front().unwrap();
                let parent_transform = self.tiles[parent_idx].transform;
                let parent_address = self.tiles[parent_idx].address.clone();
                let parent_depth = self.tiles[parent_idx].depth;

                for dir in 0u8..8 {
                    let child_transform = parent_transform.compose(&self.neighbor_xforms[dir as usize]);
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
                        extra_elevation: 0.0,
                    };
                    let child_idx = self.tiles.len();
                    self.tiles.push(child);
                    self.frontier.push_back(child_idx);
                }
            }
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
            for dir in 0u8..8 {
                let child_transform =
                    parent_transform.compose(&self.neighbor_xforms[dir as usize]);
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
                    extra_elevation: 0.0,
                };
                let child_idx = self.tiles.len();
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
        let d = center_to_center_distance();
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

        // Rebuild seen set from all tiles
        self.seen.clear();
        for tile in &self.tiles {
            let center = tile.transform.apply(Complex::ZERO);
            self.seen.insert(spatial_key(center));
        }

        // Rebuild frontier: tiles with any missing neighbor
        self.frontier.clear();
        for (idx, tile) in self.tiles.iter().enumerate() {
            let is_boundary = (0u8..8).any(|dir| {
                let neighbor = tile.transform.compose(&self.neighbor_xforms[dir as usize]);
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
/// Each step composes the corresponding neighbor transform, so the result
/// depends only on the immutable address — no accumulated drift.
fn compute_transform_from_address(address: &[u8], neighbor_xforms: &[Mobius; 8]) -> Mobius {
    let mut t = Mobius::identity();
    for &dir in address {
        t = t.compose(&neighbor_xforms[dir as usize]);
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

    #[test]
    fn test_origin_tile() {
        let state = TilingState::new();
        assert_eq!(state.tiles.len(), 1);
        assert!(state.tiles[0].address.is_empty());
        assert_eq!(state.tiles[0].depth, 0);
    }

    #[test]
    fn test_expand_depth_1() {
        let mut state = TilingState::new();
        state.expand(1);
        // Origin + 8 neighbors
        assert_eq!(state.tiles.len(), 9);
        for tile in &state.tiles[1..] {
            assert_eq!(tile.depth, 1);
            assert_eq!(tile.address.len(), 1);
        }
    }

    #[test]
    fn test_expand_depth_3() {
        let mut state = TilingState::new();
        state.expand(3);
        // {8,3}: 1 + 8 + 8*7 + 8*7*7 should be around 57 (with dedup reducing it)
        let count = state.tiles.len();
        println!("depth-3 tile count: {count}");
        assert!(count > 20, "too few tiles: {count}");
        assert!(count < 500, "too many tiles: {count}");
    }

    #[test]
    fn test_all_addresses_unique() {
        let mut state = TilingState::new();
        state.expand(2);
        let mut addrs: HashSet<Vec<u8>> = HashSet::new();
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
        let mut state = TilingState::new();
        state.expand(3);
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
