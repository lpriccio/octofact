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
/// Precision 1e4 (grid cell ~0.0001) safely distinguishes adjacent tile
/// centers at the frontier (~0.001 apart at depth 5) while still catching
/// multi-path duplicates from BFS cycles (position error ~1e-15 with
/// incremental transforms).
pub(crate) fn spatial_key(z: Complex) -> (i64, i64) {
    ((z.re * 1e4).round() as i64, (z.im * 1e4).round() as i64)
}

/// Reduce a tile address using {p,q} group relations.
/// Two rules, applied in a loop for cascading reductions:
/// 1. Inverse cancellation: last two elements d, (d+p/2)%p cancel (cross edge and back, even p only).
/// 2. Vertex cycle: last q elements [v, v+1, ..., v+q-1] mod p cancel (walk around vertex).
pub(crate) fn reduce_address(mut addr: TileAddr, p: u32, q: u32) -> TileAddr {
    let p = p as u8;
    let q = q as usize;
    let half_p = p / 2; // inverse offset (only valid for even p)
    loop {
        let len = addr.len();
        // Inverse cancellation: d, (d + p/2) % p (even p only)
        if p.is_multiple_of(2) && len >= 2 && (addr[len - 2] + half_p) % p == addr[len - 1] {
            addr.truncate(len - 2);
            continue;
        }
        // Vertex cycle: [v, v+1, ..., v+q-1] mod 4 (only valid for p=4)
        // For p=4, the vertex walk increments direction by 1 each step because
        // the opposite edge offset (p/2=2) minus 1 equals 1.
        if p == 4 && len >= q {
            let start = len - q;
            let v = addr[start];
            if (0..q).all(|i| addr[start + i] == (v + i as u8) % 4) {
                addr.truncate(start);
                continue;
            }
        }
        break;
    }
    addr
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
                let child_address = reduce_address(child_address, self.cfg.p, self.cfg.q);
                let child = Tile {
                    address: child_address.clone(),
                    transform: child_transform,
                    depth: child_address.len(),
                    parity: child_address.len() % 2 == 1,
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

        // Cap at 5 rounds per call to avoid frame spikes after eviction.
        // Any remaining expansion happens over subsequent frames.
        for _ in 0..5 {
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

    /// Find the tile index whose center is nearest to `pos`.
    /// Checks the exact spatial grid cell plus its 8 neighbors to handle
    /// floating-point drift where two computations of the same tile center
    /// may land in adjacent grid cells.
    pub fn find_tile_near(&self, pos: Complex) -> Option<usize> {
        let (kx, ky) = spatial_key(pos);
        // Try exact cell first (common case)
        if let Some(&idx) = self.spatial_to_tile.get(&(kx, ky)) {
            return Some(idx);
        }
        // Search 8 neighbors for floating-point robustness
        let mut best: Option<(usize, f64)> = None;
        for dx in -1..=1_i64 {
            for dy in -1..=1_i64 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if let Some(&idx) = self.spatial_to_tile.get(&(kx + dx, ky + dy)) {
                    let c = self.tiles[idx].transform.apply(Complex::ZERO);
                    let dist_sq = (c.re - pos.re).powi(2) + (c.im - pos.im).powi(2);
                    if best.is_none_or(|(_, d)| dist_sq < d) {
                        best = Some((idx, dist_sq));
                    }
                }
            }
        }
        best.map(|(idx, _)| idx)
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
    /// Applies the center tile's inverse transform to all tiles incrementally,
    /// keeping Mobius values at O(1) scale (no catastrophic cancellation from
    /// recomputing long address chains where |a| grows exponentially).
    /// Evicts tiles whose disk center exceeds `EVICTION_RADIUS` to prevent
    /// unbounded growth and floating-point overflow in distant transforms.
    /// Returns the new index of the center tile after compaction.
    pub fn recenter_on(&mut self, center_idx: usize) -> usize {
        const EVICTION_RADIUS: f64 = 0.99;
        let threshold_sq = EVICTION_RADIUS * EVICTION_RADIUS;

        let inv_center = self.tiles[center_idx].transform.inverse();

        // Single fused pass: transform, evict, find center, build spatial index.
        // Each tile's center is computed exactly once (was 3x before).
        self.seen.clear();
        self.spatial_to_tile.clear();
        let mut write = 0usize;
        let mut new_center_idx = 0usize;
        let mut best_dist_sq = f64::MAX;
        let mut any_evicted = false;

        for read in 0..self.tiles.len() {
            // Apply inverse transform
            self.tiles[read].transform = inv_center.compose(&self.tiles[read].transform);
            let center = self.tiles[read].transform.apply(Complex::ZERO);
            let dist_sq = center.norm_sq();

            // Eviction check
            if dist_sq > threshold_sq {
                any_evicted = true;
                continue;
            }

            // Track center tile (nearest to origin)
            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                new_center_idx = write;
            }

            // Build spatial index inline
            let key = spatial_key(center);
            self.seen.insert(key);
            self.spatial_to_tile.insert(key, write);

            // Compact in-place
            if write != read {
                self.tiles.swap(write, read);
            }
            write += 1;
        }
        self.tiles.truncate(write);

        // Only rebuild frontier when tiles were actually evicted.
        if any_evicted {
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

        new_center_idx
    }
}

/// Recompute a tile's absolute Mobius transform from its canonical address.
/// Each step composes the corresponding neighbor transform, alternating parity.
/// Note: only accurate for short addresses (~<25 steps). For longer addresses,
/// |a| grows exponentially causing catastrophic cancellation when converting
/// back to a local frame.
#[cfg(test)]
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

    #[test]
    fn test_vertex_closure_45() {
        // For {4,5}: q=5 tiles meet at vertex v0 (angle π/4), shared by edges 0 and 1.
        // Walking the cycle [0,1,2,3,0] around the vertex (crossing these edge
        // directions sequentially) should return to the same position.
        let cfg = TilingConfig::new(4, 5);
        let xforms = neighbor_transforms(&cfg);
        let dirs = [0, 1, 2, 3, 0]; // 5 edges for q=5
        let mut product = Mobius::identity();
        for &d in &dirs {
            product = product.compose(&xforms[0][d]);
        }
        let result = product.apply(Complex::ZERO);
        assert!(
            result.abs() < 1e-6,
            "vertex cycle should return to origin, got |z| = {}",
            result.abs(),
        );
        // b ≈ 0 means no translation (only rotation, which is expected)
        assert!(
            product.b.abs() < 1e-6,
            "vertex cycle b should be ~0, got ({}, {})",
            product.b.re,
            product.b.im
        );
    }

    #[test]
    fn test_vertex_closure_45_all_vertices() {
        // Check all 4 vertices of the origin square.
        let cfg = TilingConfig::new(4, 5);
        let xforms = neighbor_transforms(&cfg);
        for v in 0..4u8 {
            let dirs: Vec<usize> = (0..5).map(|i| ((v as usize + i) % 4) as usize).collect();
            let mut product = Mobius::identity();
            for &d in &dirs {
                product = product.compose(&xforms[0][d]);
            }
            let result = product.apply(Complex::ZERO);
            assert!(
                result.abs() < 1e-6,
                "vertex {v} cycle {:?} failed: |z| = {}",
                dirs,
                result.abs()
            );
        }
    }

    #[test]
    fn test_no_duplicates_after_walk() {
        // Walk 40 steps in direction 0, recentering at each step.
        // Then check for position collisions among all tiles.
        let cfg = TilingConfig::new(4, 5);
        let mut state = TilingState::new(cfg);
        state.ensure_coverage(Complex::ZERO, 5);

        for step in 0..40 {
            // Check for NaN before proceeding
            for (i, tile) in state.tiles.iter().enumerate() {
                let c = tile.transform.apply(Complex::ZERO);
                assert!(
                    !c.re.is_nan() && !c.im.is_nan(),
                    "NaN in tile {i} (addr len {}) at step {step}, a=({},{}) b=({},{})",
                    tile.address.len(),
                    tile.transform.a.re, tile.transform.a.im,
                    tile.transform.b.re, tile.transform.b.im,
                );
            }
            // Find neighbor across edge 0 from the tile nearest to origin
            let center_idx = state
                .tiles
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = a.transform.apply(Complex::ZERO).norm_sq();
                    let db = b.transform.apply(Complex::ZERO).norm_sq();
                    da.partial_cmp(&db).unwrap()
                })
                .unwrap()
                .0;
            let parity = state.tiles[center_idx].parity as usize;
            let xforms = &state.neighbor_xforms[parity];
            let neighbor_center = state.tiles[center_idx]
                .transform
                .compose(&xforms[0])
                .apply(Complex::ZERO);
            let neighbor_idx = state.find_tile_near(neighbor_center);
            if let Some(idx) = neighbor_idx {
                state.recenter_on(idx);
                state.ensure_coverage(Complex::ZERO, 5);
            } else {
                panic!("neighbor tile not found at step {step}, tiles: {}, center tile pos: ({},{})",
                    state.tiles.len(),
                    state.tiles[center_idx].transform.apply(Complex::ZERO).re,
                    state.tiles[center_idx].transform.apply(Complex::ZERO).im,
                );
            }
        }

        // Scan all tiles for position collisions (duplicates).
        let epsilon = 1e-4;
        let mut collision_count = 0;
        let centers: Vec<Complex> = state
            .tiles
            .iter()
            .map(|t| t.transform.apply(Complex::ZERO))
            .collect();
        for i in 0..centers.len() {
            for j in (i + 1)..centers.len() {
                let dist = (centers[i] - centers[j]).abs();
                if dist < epsilon {
                    collision_count += 1;
                }
            }
        }
        assert_eq!(
            collision_count, 0,
            "found {collision_count} duplicate tile pairs after walking 40 steps"
        );
    }

    #[test]
    fn test_no_nan_after_long_walk() {
        // Walk 60 steps — verify no NaN in any tile transform.
        // (The old address-based recenter produced NaN at ~40 steps.)
        let cfg = TilingConfig::new(4, 5);
        let mut state = TilingState::new(cfg);
        state.ensure_coverage(Complex::ZERO, 3);

        for _ in 0..60 {
            let center_idx = state
                .tiles
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = a.transform.apply(Complex::ZERO).norm_sq();
                    let db = b.transform.apply(Complex::ZERO).norm_sq();
                    da.partial_cmp(&db).unwrap()
                })
                .unwrap()
                .0;
            let parity = state.tiles[center_idx].parity as usize;
            let xforms = &state.neighbor_xforms[parity];
            let neighbor_center = state.tiles[center_idx]
                .transform
                .compose(&xforms[0])
                .apply(Complex::ZERO);
            if let Some(idx) = state.find_tile_near(neighbor_center) {
                state.recenter_on(idx);
                state.ensure_coverage(Complex::ZERO, 3);
            }
        }

        for tile in &state.tiles {
            let c = tile.transform.apply(Complex::ZERO);
            assert!(
                !c.re.is_nan() && !c.im.is_nan(),
                "NaN in tile {:?} after 60-step walk",
                tile.address
            );
            assert!(
                c.abs() < 1.0,
                "tile {:?} center outside disk: {}",
                tile.address,
                c.abs()
            );
        }
    }

    #[test]
    fn test_recenter_returns_center_near_origin() {
        let cfg = TilingConfig::new(4, 5);
        let mut state = TilingState::new(cfg);
        state.ensure_coverage(Complex::ZERO, 3);
        // Recenter on a non-origin tile
        let new_idx = state.recenter_on(1);
        // The recentered tile should now be near origin
        let pos = state.tiles[new_idx].transform.apply(Complex::ZERO);
        assert!(pos.abs() < 0.01, "recentered tile should be near origin, got |z| = {}", pos.abs());
    }

    #[test]
    fn test_reduce_inverse_cancellation() {
        // d, (d+2)%4 should cancel for all 4 pairs
        for d in 0..4u8 {
            let addr: TileAddr = smallvec::smallvec![d, (d + 2) % 4];
            let reduced = reduce_address(addr, 4, 5);
            assert!(reduced.is_empty(), "expected [] for [{}, {}], got {:?}", d, (d + 2) % 4, reduced);
        }
    }

    #[test]
    fn test_reduce_no_false_positive() {
        // Adjacent directions (not inverses) should not cancel
        let addr: TileAddr = smallvec::smallvec![0, 1];
        let reduced = reduce_address(addr, 4, 5);
        assert_eq!(reduced.as_slice(), &[0, 1]);
    }

    #[test]
    fn test_reduce_vertex_cycle_q5() {
        // [v, v+1, v+2, v+3, v+4 mod 4] should cancel for all starting vertices
        for v in 0..4u8 {
            let addr: TileAddr = (0..5).map(|i| (v + i) % 4).collect();
            let reduced = reduce_address(addr.clone(), 4, 5);
            assert!(reduced.is_empty(), "expected [] for vertex cycle starting at {v}, got {:?}", reduced);
        }
    }

    #[test]
    fn test_reduce_cascading() {
        // [2, 0, 0, 2] — inner pair [0, 2] cancels first, leaving [2, 0],
        // then [2, 0] cancels (2+2=4≡0).
        let addr: TileAddr = smallvec::smallvec![2, 0, 0, 2];
        let reduced = reduce_address(addr, 4, 5);
        assert!(reduced.is_empty(), "expected [] for cascading cancellation, got {:?}", reduced);
    }

    #[test]
    fn test_reduce_vertex_cycle_with_prefix() {
        // [2, 0, 1, 2, 3, 0] — the last 5 form a vertex cycle, leaving [2]
        let addr: TileAddr = smallvec::smallvec![2, 0, 1, 2, 3, 0];
        let reduced = reduce_address(addr, 4, 5);
        assert_eq!(reduced.as_slice(), &[2], "expected [2], got {:?}", reduced);
    }

    #[test]
    fn test_walk_and_return_canonical_origin() {
        // Walk 10 steps east (dir 0), then 10 steps west (dir 2) = back to origin.
        // After evict/re-expand, the origin tile should have address [].
        let cfg = TilingConfig::new(4, 5);
        let mut state = TilingState::new(cfg);
        state.ensure_coverage(Complex::ZERO, 5);

        // Walk east 10 steps
        for _ in 0..10 {
            let center_idx = state
                .tiles
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = a.transform.apply(Complex::ZERO).norm_sq();
                    let db = b.transform.apply(Complex::ZERO).norm_sq();
                    da.partial_cmp(&db).unwrap()
                })
                .unwrap()
                .0;
            let parity = state.tiles[center_idx].parity as usize;
            let neighbor_center = state.tiles[center_idx]
                .transform
                .compose(&state.neighbor_xforms[parity][0])
                .apply(Complex::ZERO);
            if let Some(idx) = state.find_tile_near(neighbor_center) {
                state.recenter_on(idx);
                state.ensure_coverage(Complex::ZERO, 5);
            }
        }

        // Walk west 10 steps (dir 2 = opposite of 0)
        for _ in 0..10 {
            let center_idx = state
                .tiles
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = a.transform.apply(Complex::ZERO).norm_sq();
                    let db = b.transform.apply(Complex::ZERO).norm_sq();
                    da.partial_cmp(&db).unwrap()
                })
                .unwrap()
                .0;
            let parity = state.tiles[center_idx].parity as usize;
            let neighbor_center = state.tiles[center_idx]
                .transform
                .compose(&state.neighbor_xforms[parity][2])
                .apply(Complex::ZERO);
            if let Some(idx) = state.find_tile_near(neighbor_center) {
                state.recenter_on(idx);
                state.ensure_coverage(Complex::ZERO, 5);
            }
        }

        // The tile nearest origin should have address []
        let center_idx = state
            .tiles
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = a.transform.apply(Complex::ZERO).norm_sq();
                let db = b.transform.apply(Complex::ZERO).norm_sq();
                da.partial_cmp(&db).unwrap()
            })
            .unwrap()
            .0;
        let origin_addr = &state.tiles[center_idx].address;
        assert!(
            origin_addr.is_empty(),
            "origin tile after walk-and-return should have address [], got {:?}",
            origin_addr
        );
    }
}
