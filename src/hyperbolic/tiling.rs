use std::collections::{HashMap, HashSet, VecDeque};

use smallvec::SmallVec;

use super::cell_graph::word_to_mobius;
use super::cell_id::{self, CellId};
use super::poincare::{Complex, Mobius, TilingConfig, center_to_center_distance, neighbor_transforms, poincare_distance};
use super::rewrite::{self, RewriteRule, A, B, B_INV};

/// Legacy tile address type. Kept for backward compatibility with world.rs
/// and power.rs which will be migrated to CellId in Phase 6.
pub type TileAddr = SmallVec<[u8; 12]>;

/// Maximum number of tiles to keep. Prevents unbounded growth in hyperbolic space.
const MAX_TILES: usize = 4096;

/// A tile in the {4,5} tiling, identified by its canonical CellId.
#[derive(Clone, Debug)]
pub struct Tile {
    /// Canonical algebraic cell identity (shortlex-minimum reduced word).
    pub id: CellId,
    /// Mobius transform mapping the canonical polygon to this tile's position.
    pub transform: Mobius,
    /// Parity: false = even (same orientation as origin), true = odd (flipped).
    pub parity: bool,
    /// Physical edge index the turtle faces after walking the canonical word.
    /// Used during construction to compute cached neighbor CellIds.
    #[allow(dead_code)]
    facing: u8,
    /// Cached CellIds of the 4 neighbors (by physical edge index).
    /// Computed once when the tile is created, avoiding repeated K-B reduction.
    pub neighbors: [CellId; 4],
}

/// Spatial dedup key: discretize Poincare disk position to grid.
/// Used as a secondary index for click detection / spatial lookup.
fn spatial_key(z: Complex) -> (i64, i64) {
    ((z.re * 1e4).round() as i64, (z.im * 1e4).round() as i64)
}

/// Compute turtle facing direction and parity from a word.
/// Facing: the physical edge index (0-3) the turtle faces after walking the word.
/// Parity: true if odd number of 'a' letters (edge crossings).
fn word_facing_parity(word: &[u8]) -> (u8, bool) {
    let mut facing: u8 = 0;
    let mut parity = false;
    for &letter in word {
        match letter {
            A => {
                parity = !parity;
                facing = (facing + 2) % 4;
            }
            B => {
                facing = (facing + 1) % 4;
            }
            B_INV => {
                facing = (facing + 3) % 4;
            }
            _ => unreachable!(),
        }
    }
    (facing, parity)
}

/// Compute all 4 neighbor CellIds for a tile, mapping physical edges to algebraic edges.
fn compute_neighbors(id: &CellId, facing: u8, rules: &[RewriteRule]) -> [CellId; 4] {
    [0u8, 1, 2, 3].map(|dir| {
        let cell_edge = (dir + 4 - facing) % 4;
        cell_id::neighbor(id, cell_edge, rules).id
    })
}

/// BFS tiling state for incremental expansion of a {4,5} tiling.
/// Uses algebraic CellId for exact cell identity (no floating-point drift).
pub struct TilingState {
    pub cfg: TilingConfig,
    pub tiles: Vec<Tile>,
    /// Exact dedup: canonical CellIds of all known tiles.
    seen: HashSet<CellId>,
    /// Primary index: CellId → tile index in `tiles` Vec.
    id_to_tile: HashMap<CellId, usize>,
    /// Secondary spatial index for click detection and `find_tile_near()`.
    spatial_to_tile: HashMap<(i64, i64), usize>,
    frontier: VecDeque<usize>,
    /// `[0]` = transforms for even-parity tiles, `[1]` = for odd-parity tiles.
    pub neighbor_xforms: [Vec<Mobius>; 2],
    /// Cached confluent rewrite rules for {4,q}.
    rules: Vec<RewriteRule>,
    /// View offset: maps absolute (word_to_mobius) coordinates to view-relative coordinates.
    /// Recomputed on each recenter to avoid holonomy accumulation.
    view_offset: Mobius,
}

impl TilingState {
    pub fn new(cfg: TilingConfig) -> Self {
        assert_eq!(cfg.p, 4, "CellId tiling only supports {{4,q}} (got p={})", cfg.p);

        let rules = rewrite::load_rules(cfg.q);
        let origin_id = CellId::origin();
        let origin_neighbors = compute_neighbors(&origin_id, 0, &rules);
        let origin = Tile {
            id: origin_id.clone(),
            transform: Mobius::identity(),
            parity: false,
            facing: 0,
            neighbors: origin_neighbors,
        };

        let key = spatial_key(Complex::ZERO);
        let mut seen = HashSet::new();
        seen.insert(origin_id.clone());
        let mut id_to_tile = HashMap::new();
        id_to_tile.insert(origin_id, 0);
        let mut spatial_to_tile = HashMap::new();
        spatial_to_tile.insert(key, 0);

        let mut frontier = VecDeque::new();
        frontier.push_back(0);

        Self {
            cfg,
            tiles: vec![origin],
            seen,
            id_to_tile,
            spatial_to_tile,
            frontier,
            neighbor_xforms: neighbor_transforms(&cfg),
            rules,
            view_offset: Mobius::identity(),
        }
    }

    /// Create a tiling centered on the given CellId instead of the origin.
    /// The target cell will be at the center of the Poincare disk (index 0).
    pub fn new_centered_on(cfg: TilingConfig, center_id: &CellId) -> Self {
        assert_eq!(cfg.p, 4, "CellId tiling only supports {{4,q}} (got p={})", cfg.p);

        let rules = rewrite::load_rules(cfg.q);
        let xforms = neighbor_transforms(&cfg);

        let (facing, parity) = word_facing_parity(center_id.word());
        let absolute = word_to_mobius(center_id.word(), &xforms);
        let view_offset = absolute.inverse();
        // Center tile's view-space transform is view_offset * absolute ≈ identity
        let transform = view_offset.compose(&absolute);
        let neighbors = compute_neighbors(center_id, facing, &rules);

        let tile = Tile {
            id: center_id.clone(),
            transform,
            parity,
            facing,
            neighbors,
        };

        let key = spatial_key(transform.apply(Complex::ZERO));
        let mut seen = HashSet::new();
        seen.insert(center_id.clone());
        let mut id_to_tile = HashMap::new();
        id_to_tile.insert(center_id.clone(), 0);
        let mut spatial_to_tile = HashMap::new();
        spatial_to_tile.insert(key, 0);

        let mut frontier = VecDeque::new();
        frontier.push_back(0);

        Self {
            cfg,
            tiles: vec![tile],
            seen,
            id_to_tile,
            spatial_to_tile,
            frontier,
            neighbor_xforms: xforms,
            rules,
            view_offset,
        }
    }

    /// Expand only frontier tiles within `max_dist` hyperbolic distance of `target`.
    /// Uses algebraic CellId for dedup (exact, immune to floating-point drift).
    /// Stops early if the tile count reaches MAX_TILES.
    pub fn expand_near(&mut self, target: Complex, max_dist: f64) {
        let frontier_len = self.frontier.len();
        if frontier_len == 0 {
            return;
        }
        let mut deferred = VecDeque::new();
        for _ in 0..frontier_len {
            if self.tiles.len() >= MAX_TILES {
                // Drain remaining frontier items into deferred.
                while let Some(idx) = self.frontier.pop_front() {
                    deferred.push_back(idx);
                }
                break;
            }
            let parent_idx = self.frontier.pop_front().unwrap();
            let parent_center = self.tiles[parent_idx].transform.apply(Complex::ZERO);
            if poincare_distance(parent_center, target) > max_dist {
                deferred.push_back(parent_idx);
                continue;
            }
            // Use cached neighbor CellIds from the parent tile.
            let parent_neighbors = self.tiles[parent_idx].neighbors.clone();
            for dir in 0..4u8 {
                let child_id = &parent_neighbors[dir as usize];
                if self.seen.contains(child_id) {
                    continue;
                }
                // Compute child transform from canonical word (via word_to_mobius),
                // not incremental composition, to avoid holonomy accumulation.
                let child_transform = self.view_offset
                    .compose(&word_to_mobius(child_id.word(), &self.neighbor_xforms));
                let (child_facing, child_parity) = word_facing_parity(child_id.word());
                let child_neighbors = compute_neighbors(child_id, child_facing, &self.rules);
                let child = Tile {
                    id: child_id.clone(),
                    transform: child_transform,
                    parity: child_parity,
                    facing: child_facing,
                    neighbors: child_neighbors,
                };
                let child_idx = self.tiles.len();
                self.seen.insert(child_id.clone());
                self.id_to_tile.insert(child_id.clone(), child_idx);
                let center = child_transform.apply(Complex::ZERO);
                self.spatial_to_tile.insert(spatial_key(center), child_idx);
                self.tiles.push(child);
                self.frontier.push_back(child_idx);
            }
        }
        while let Some(idx) = deferred.pop_back() {
            self.frontier.push_front(idx);
        }
    }

    /// Expand BFS until every frontier tile is at least `min_layers` hops away from `target`.
    pub fn ensure_coverage(&mut self, target: Complex, min_layers: usize) {
        let d = center_to_center_distance(&self.cfg);
        let required_dist = (min_layers as f64 + 0.5) * d;
        for _ in 0..5 {
            if self.frontier.is_empty() {
                break;
            }
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

    /// Find the tile index whose center is nearest to `pos` (spatial lookup).
    /// Used for click detection and camera movement.
    pub fn find_tile_near(&self, pos: Complex) -> Option<usize> {
        let (kx, ky) = spatial_key(pos);
        if let Some(&idx) = self.spatial_to_tile.get(&(kx, ky)) {
            return Some(idx);
        }
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

    /// Look up a tile by its CellId.
    #[allow(dead_code)]
    pub fn find_tile(&self, id: &CellId) -> Option<usize> {
        self.id_to_tile.get(id).copied()
    }

    /// Find the CellId of the tile adjacent to `tile_idx` across physical edge `edge` (0..3).
    /// Returns None if the neighbor tile hasn't been expanded yet.
    /// Uses cached neighbor CellIds — no K-B reduction needed.
    pub fn neighbor_tile_id(&self, tile_idx: usize, edge: u8) -> Option<CellId> {
        let neighbor_id = &self.tiles[tile_idx].neighbors[edge as usize];
        if self.seen.contains(neighbor_id) {
            Some(neighbor_id.clone())
        } else {
            None
        }
    }

    /// Recenter the tiling so that `center_idx` becomes the origin.
    /// CellIds are absolute — they don't change on recenter. Only Mobius transforms
    /// are updated (recomputed from word_to_mobius to avoid holonomy drift).
    /// Evicts tiles beyond EVICTION_RADIUS to prevent unbounded growth.
    /// Returns the new index of the center tile after compaction.
    pub fn recenter_on(&mut self, center_idx: usize) -> usize {
        const EVICTION_RADIUS: f64 = 0.99;
        let threshold_sq = EVICTION_RADIUS * EVICTION_RADIUS;

        // Recompute view_offset from the center tile's canonical word.
        // This avoids accumulating floating-point error across recenters.
        let center_absolute = word_to_mobius(
            self.tiles[center_idx].id.word(),
            &self.neighbor_xforms,
        );
        self.view_offset = center_absolute.inverse();

        // Recompute all tile transforms from canonical words + view_offset.
        // Then evict, compact, and rebuild indices in a single pass.
        self.id_to_tile.clear();
        self.spatial_to_tile.clear();
        let mut write = 0usize;
        let mut new_center_idx = 0usize;
        let mut best_dist_sq = f64::MAX;
        let mut any_evicted = false;

        for read in 0..self.tiles.len() {
            let absolute = word_to_mobius(self.tiles[read].id.word(), &self.neighbor_xforms);
            self.tiles[read].transform = self.view_offset.compose(&absolute);
            let center = self.tiles[read].transform.apply(Complex::ZERO);
            let dist_sq = center.norm_sq();

            if dist_sq > threshold_sq {
                any_evicted = true;
                continue;
            }

            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                new_center_idx = write;
            }

            // Rebuild indices
            self.id_to_tile.insert(self.tiles[read].id.clone(), write);
            self.spatial_to_tile.insert(spatial_key(center), write);

            if write != read {
                self.tiles.swap(write, read);
            }
            write += 1;
        }
        self.tiles.truncate(write);

        // Rebuild seen and frontier when tiles were actually evicted.
        if any_evicted {
            self.seen.clear();
            for tile in &self.tiles {
                self.seen.insert(tile.id.clone());
            }

            self.frontier.clear();
            for (idx, tile) in self.tiles.iter().enumerate() {
                // Use cached neighbor CellIds — no K-B reduction needed.
                let is_boundary = tile.neighbors.iter().any(|n| !self.seen.contains(n));
                if is_boundary {
                    self.frontier.push_back(idx);
                }
            }
        }

        new_center_idx
    }
}

/// Format a CellId for display (turtle word notation).
pub fn format_cell_id(id: &CellId) -> String {
    if id.is_empty() {
        "O".to_string()
    } else {
        let s = id.to_string();
        if s.len() > 8 {
            format!("..{}", &s[s.len() - 6..])
        } else {
            s
        }
    }
}

/// Format a tile address for display (legacy, for backward compatibility).
#[allow(dead_code)]
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

    fn cfg45() -> TilingConfig {
        TilingConfig::new(4, 5)
    }

    #[test]
    fn test_origin_tile() {
        let state = TilingState::new(cfg45());
        assert_eq!(state.tiles.len(), 1);
        assert!(state.tiles[0].id.is_empty());
    }

    #[test]
    fn test_ensure_coverage_depth_1() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 1);
        assert!(state.tiles.len() > 1, "should have more than just the origin");
    }

    #[test]
    fn test_ensure_coverage_depth_3() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);
        let count = state.tiles.len();
        assert!(count > 20, "too few tiles: {count}");
        assert!(count < 2000, "too many tiles: {count}");
    }

    #[test]
    fn test_all_cell_ids_unique() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 2);
        let mut ids: HashSet<CellId> = HashSet::new();
        for tile in &state.tiles {
            assert!(
                ids.insert(tile.id.clone()),
                "duplicate CellId: {:?}",
                tile.id
            );
        }
    }

    #[test]
    fn test_all_centers_inside_disk() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);
        for tile in &state.tiles {
            let c = tile.transform.apply(Complex::ZERO);
            assert!(
                c.abs() < 1.0,
                "tile {:?} center outside disk: {}",
                tile.id,
                c.abs()
            );
        }
    }

    #[test]
    fn test_format_cell_id() {
        assert_eq!(format_cell_id(&CellId::origin()), "O");
        let r = rewrite::load_rules(5);
        let word = rewrite::string_to_word("a");
        let oc = cell_id::canonicalize(&word, &r);
        assert!(!format_cell_id(&oc.id).is_empty());
    }

    #[test]
    fn test_vertex_closure_45() {
        let cfg = cfg45();
        let xforms = neighbor_transforms(&cfg);
        let dirs = [0, 1, 2, 3, 0];
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
        assert!(
            product.b.abs() < 1e-6,
            "vertex cycle b should be ~0, got ({}, {})",
            product.b.re,
            product.b.im
        );
    }

    #[test]
    fn test_vertex_closure_45_all_vertices() {
        let cfg = cfg45();
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
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);

        for step in 0..10 {
            for (i, tile) in state.tiles.iter().enumerate() {
                let c = tile.transform.apply(Complex::ZERO);
                assert!(
                    !c.re.is_nan() && !c.im.is_nan(),
                    "NaN in tile {i} ({:?}) at step {step}",
                    tile.id,
                );
            }
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
                state.ensure_coverage(Complex::ZERO, 3);
            } else {
                panic!("neighbor tile not found at step {step}");
            }
        }

        // Verify no CellId duplicates
        let mut ids: HashSet<CellId> = HashSet::new();
        for tile in &state.tiles {
            assert!(ids.insert(tile.id.clone()), "duplicate CellId after walk: {:?}", tile.id);
        }

        // Also verify no position collisions
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
            "found {collision_count} duplicate tile pairs after walking 10 steps"
        );
    }

    #[test]
    fn test_no_nan_after_long_walk() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);

        for _ in 0..15 {
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
                "NaN in tile {:?} after 15-step walk",
                tile.id
            );
            assert!(
                c.abs() < 1.0,
                "tile {:?} center outside disk: {}",
                tile.id,
                c.abs()
            );
        }
    }

    #[test]
    fn test_recenter_returns_center_near_origin() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);
        let new_idx = state.recenter_on(1);
        let pos = state.tiles[new_idx].transform.apply(Complex::ZERO);
        assert!(pos.abs() < 0.01, "recentered tile should be near origin, got |z| = {}", pos.abs());
    }

    // --- Phase 5 specific tests ---

    #[test]
    fn test_mobius_accuracy_before_recenter() {
        use super::super::cell_graph::word_to_mobius;
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);

        // Every tile's stored Mobius should match word_to_mobius (both center and rotation).
        let test_pt = Complex::new(0.1, 0.05);
        let mut bad_count = 0;
        for tile in &state.tiles {
            let ref_mob = word_to_mobius(tile.id.word(), &state.neighbor_xforms);
            let pt_dist = (ref_mob.apply(test_pt) - tile.transform.apply(test_pt)).abs();
            if pt_dist > 1e-6 {
                bad_count += 1;
            }
        }
        assert_eq!(bad_count, 0, "{bad_count}/{} tiles have incorrect Mobius", state.tiles.len());
    }

    #[test]
    fn test_mobius_accuracy_after_recenter() {
        use super::super::cell_graph::word_to_mobius;
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);
        state.recenter_on(1);

        // After recenter, stored = view_offset ∘ word_to_mobius(tile).
        // Check via the test-point method.
        let test_pt = Complex::new(0.1, 0.05);
        let mut bad_count = 0;
        for tile in &state.tiles {
            let expected = state.view_offset
                .compose(&word_to_mobius(tile.id.word(), &state.neighbor_xforms));
            let dist = (expected.apply(test_pt) - tile.transform.apply(test_pt)).abs();
            if dist > 1e-6 {
                bad_count += 1;
            }
        }
        assert_eq!(bad_count, 0, "{bad_count} tiles have incorrect Mobius after recenter");
    }

    #[test]
    fn test_mobius_accuracy_after_recenter_and_expand() {
        use super::super::cell_graph::word_to_mobius;
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);

        // Walk east: recenter on neighbor, re-expand
        let parity = state.tiles[0].parity as usize;
        let neighbor_center = state.tiles[0]
            .transform
            .compose(&state.neighbor_xforms[parity][0])
            .apply(Complex::ZERO);
        let idx = state.find_tile_near(neighbor_center).expect("neighbor not found");
        state.recenter_on(idx);
        state.ensure_coverage(Complex::ZERO, 3);

        // All tiles should match view_offset ∘ word_to_mobius.
        let test_pt = Complex::new(0.1, 0.05);
        let mut bad_count = 0;
        for tile in &state.tiles {
            let expected = state.view_offset
                .compose(&word_to_mobius(tile.id.word(), &state.neighbor_xforms));
            let dist = (expected.apply(test_pt) - tile.transform.apply(test_pt)).abs();
            if dist > 1e-6 {
                bad_count += 1;
            }
        }
        assert_eq!(bad_count, 0,
            "{bad_count}/{} tiles have incorrect Mobius after recenter+expand", state.tiles.len());
    }

    #[test]
    fn test_walk_east_and_back() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);

        // Walk east (physical edge 0)
        let expected_neighbor_id = state.tiles[0].neighbors[0].clone();
        let parity = state.tiles[0].parity as usize;
        let neighbor_center = state.tiles[0]
            .transform
            .compose(&state.neighbor_xforms[parity][0])
            .apply(Complex::ZERO);
        let idx = state.find_tile_near(neighbor_center).expect("neighbor not found");
        assert_eq!(state.tiles[idx].id, expected_neighbor_id,
            "spatial lookup should find the algebraic neighbor");

        // Recenter on the neighbor
        state.recenter_on(idx);
        state.ensure_coverage(Complex::ZERO, 3);

        // Find new center, walk west (physical edge 2)
        let new_center_idx = state.tiles.iter().enumerate()
            .min_by(|(_, a), (_, b)| {
                a.transform.apply(Complex::ZERO).norm_sq()
                    .partial_cmp(&b.transform.apply(Complex::ZERO).norm_sq()).unwrap()
            }).unwrap().0;
        let expected_back_id = state.tiles[new_center_idx].neighbors[2].clone();
        let parity2 = state.tiles[new_center_idx].parity as usize;
        let back_center = state.tiles[new_center_idx]
            .transform
            .compose(&state.neighbor_xforms[parity2][2])
            .apply(Complex::ZERO);
        let back_idx = state.find_tile_near(back_center).expect("back neighbor not found");
        assert_eq!(state.tiles[back_idx].id, expected_back_id,
            "spatial lookup for west should find the algebraic neighbor");
    }

    #[test]
    fn test_walk_and_return_origin_cellid() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);

        // Walk 15 steps east (physical edge 0)
        for _ in 0..15 {
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
                state.ensure_coverage(Complex::ZERO, 3);
            }
        }

        // Walk 15 steps west (physical edge 2 = opposite of 0)
        for _ in 0..15 {
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
                state.ensure_coverage(Complex::ZERO, 3);
            }
        }

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
        assert!(
            state.tiles[center_idx].id.is_empty(),
            "origin tile after 15+15 walk should have empty CellId, got {:?}",
            state.tiles[center_idx].id
        );
    }

    #[test]
    fn test_no_duplicate_cell_ids_after_expand_recenter() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);

        for _ in 0..8 {
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
                .compose(&state.neighbor_xforms[parity][1])
                .apply(Complex::ZERO);
            if let Some(idx) = state.find_tile_near(neighbor_center) {
                state.recenter_on(idx);
                state.ensure_coverage(Complex::ZERO, 3);
            }
        }

        let mut ids: HashSet<CellId> = HashSet::new();
        for tile in &state.tiles {
            assert!(ids.insert(tile.id.clone()), "duplicate CellId: {:?}", tile.id);
        }
    }

    #[test]
    fn test_recenter_preserves_pairwise_distances() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);

        let pre_centers: HashMap<CellId, Complex> = state.tiles.iter()
            .map(|t| (t.id.clone(), t.transform.apply(Complex::ZERO)))
            .collect();

        state.recenter_on(1);

        for t1 in &state.tiles {
            for t2 in &state.tiles {
                if t1.id == t2.id { continue; }
                if let (Some(&pre1), Some(&pre2)) = (pre_centers.get(&t1.id), pre_centers.get(&t2.id)) {
                    let pre_dist = poincare_distance(pre1, pre2);
                    let post1 = t1.transform.apply(Complex::ZERO);
                    let post2 = t2.transform.apply(Complex::ZERO);
                    let post_dist = poincare_distance(post1, post2);
                    assert!(
                        (pre_dist - post_dist).abs() < 1e-8,
                        "distance between {:?} and {:?} changed: {pre_dist} → {post_dist}",
                        t1.id, t2.id,
                    );
                }
            }
        }
    }

    #[test]
    fn test_find_tile_by_id() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 2);

        for (idx, tile) in state.tiles.iter().enumerate() {
            let found = state.find_tile(&tile.id);
            assert_eq!(found, Some(idx), "find_tile should return correct index for {:?}", tile.id);
        }
    }

    #[test]
    fn test_neighbor_tile_id() {
        let mut state = TilingState::new(cfg45());
        state.ensure_coverage(Complex::ZERO, 3);

        // Origin (index 0) should have all 4 neighbors
        for edge in 0..4u8 {
            let neighbor = state.neighbor_tile_id(0, edge);
            assert!(neighbor.is_some(), "origin should have neighbor across edge {edge}");
            let n_id = neighbor.unwrap();
            assert!(!n_id.is_empty(), "origin's neighbor should not be origin");
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
