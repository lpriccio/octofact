//! Algebraic neighbor graph for {4,5} hyperbolic tiling.
//!
//! `CellGraph` maintains a set of discovered cells with precomputed neighbor
//! relationships and Mobius transforms, using the confluent rewrite system
//! for exact algebraic cell identity.

// Module is used only in tests until Phase 5 integration.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use super::cell_id::{self, CellId};
use super::poincare::{Mobius, TilingConfig, neighbor_transforms};
use super::rewrite::{self, RewriteRule, A, B, B_INV};

/// Per-cell data in the CellGraph.
#[derive(Clone, Debug)]
pub struct CellData {
    /// Canonical cell identity.
    pub id: CellId,
    /// Direct neighbors, one per edge (0-3 in canonical frame).
    pub neighbors: [CellId; 4],
    /// For each edge e: which edge of `neighbors[e]` connects back to this cell.
    pub neighbor_orientations: [u8; 4],
    /// Mobius transform mapping origin polygon to this cell's position.
    pub mobius: Mobius,
    /// Even/odd parity (number of `a` letters in canonical word, mod 2).
    pub parity: bool,
}

/// Graph of discovered cells with precomputed neighbors and Mobius transforms.
pub struct CellGraph {
    /// All loaded cells, keyed by canonical CellId.
    pub cells: HashMap<CellId, CellData>,
    /// Cached rewrite rules for {4,q}.
    rules: Vec<RewriteRule>,
    /// Neighbor transforms for even/odd parity tiles.
    neighbor_xforms: [Vec<Mobius>; 2],
    /// The origin cell (empty word).
    pub origin: CellId,
}

impl CellGraph {
    /// Create a new CellGraph for the given tiling configuration.
    pub fn new(cfg: &TilingConfig) -> Self {
        assert_eq!(cfg.p, 4, "CellGraph only supports {{4,q}} tilings");

        let rules = rewrite::load_rules(cfg.q);
        let neighbor_xforms = neighbor_transforms(cfg);
        let origin = CellId::origin();

        let mut graph = Self {
            cells: HashMap::new(),
            rules,
            neighbor_xforms,
            origin: origin.clone(),
        };
        graph.ensure_cell(&origin);
        graph
    }

    /// BFS from `center`, expanding `radius` hops out.
    /// Ensures all cells within `radius` hops of `center` are loaded.
    pub fn expand_bfs(&mut self, center: &CellId, radius: usize) {
        self.ensure_cell(center);

        let mut ring: Vec<CellId> = vec![center.clone()];
        let mut visited: HashSet<CellId> = HashSet::new();
        visited.insert(center.clone());

        for _depth in 0..radius {
            let mut next_ring = Vec::new();
            for cell_id in &ring {
                let neighbors: [CellId; 4] = self.cells[cell_id].neighbors.clone();
                for neighbor_id in neighbors {
                    if visited.insert(neighbor_id.clone()) {
                        self.ensure_cell(&neighbor_id);
                        next_ring.push(neighbor_id);
                    }
                }
            }
            ring = next_ring;
        }
    }

    /// Ensure all cells within `radius` hops of `cell` are loaded.
    /// Idempotent: skips already-loaded cells.
    pub fn ensure_neighborhood(&mut self, cell: &CellId, radius: usize) {
        self.expand_bfs(cell, radius);
    }

    /// Return all loaded cells within `radius` BFS hops of `center`.
    /// Only includes cells that are already in the graph.
    pub fn cells_within(&self, center: &CellId, radius: usize) -> Vec<&CellId> {
        let mut result: Vec<&CellId> = Vec::new();
        let mut visited: HashSet<&CellId> = HashSet::new();

        let Some(center_data) = self.cells.get(center) else {
            return result;
        };

        visited.insert(&center_data.id);
        result.push(&center_data.id);
        let mut frontier: Vec<&CellId> = vec![&center_data.id];

        for _depth in 0..radius {
            let mut next: Vec<&CellId> = Vec::new();
            for cell_id in frontier {
                if let Some(data) = self.cells.get(cell_id) {
                    for neighbor_id in &data.neighbors {
                        if let Some(nd) = self.cells.get(neighbor_id) {
                            if visited.insert(&nd.id) {
                                result.push(&nd.id);
                                next.push(&nd.id);
                            }
                        }
                    }
                }
            }
            frontier = next;
        }

        result
    }

    /// Ensure a cell exists in the graph, computing its data if missing.
    fn ensure_cell(&mut self, cell_id: &CellId) {
        if self.cells.contains_key(cell_id) {
            return;
        }

        // Compute Mobius from canonical word.
        let mobius = word_to_mobius(cell_id.word(), &self.neighbor_xforms);

        // Compute parity: count 'a' letters in canonical word.
        let parity = cell_id.word().iter().filter(|&&b| b == A).count() % 2 == 1;

        // Compute all 4 neighbors.
        let oriented = cell_id::all_neighbors(cell_id, &self.rules);
        let neighbors = [
            oriented[0].id.clone(),
            oriented[1].id.clone(),
            oriented[2].id.clone(),
            oriented[3].id.clone(),
        ];

        // Compute back-edges: which edge of each neighbor leads back to this cell.
        let neighbor_orientations = [
            find_back_edge(&neighbors[0], cell_id, &self.rules),
            find_back_edge(&neighbors[1], cell_id, &self.rules),
            find_back_edge(&neighbors[2], cell_id, &self.rules),
            find_back_edge(&neighbors[3], cell_id, &self.rules),
        ];

        let data = CellData {
            id: cell_id.clone(),
            neighbors,
            neighbor_orientations,
            mobius,
            parity,
        };
        self.cells.insert(cell_id.clone(), data);
    }
}

/// Find which edge of `neighbor_id` leads back to `original_id`.
fn find_back_edge(neighbor_id: &CellId, original_id: &CellId, rules: &[RewriteRule]) -> u8 {
    for e in 0..4 {
        if cell_id::neighbor(neighbor_id, e, rules).id == *original_id {
            return e;
        }
    }
    panic!("no back edge from {} to {}", neighbor_id, original_id)
}

/// Compute the Mobius transform for a word by walking the turtle alphabet,
/// composing neighbor transforms for each edge crossing (`a`).
///
/// `B` (turn left) and `b` (turn right) only change the turtle's facing
/// direction without affecting the transform.
pub fn word_to_mobius(word: &[u8], neighbor_xforms: &[Vec<Mobius>; 2]) -> Mobius {
    word_to_mobius_state(word, neighbor_xforms).0
}

/// Compute Mobius, final facing, and parity from a word.
/// Used internally for incremental BFS validation.
fn word_to_mobius_state(word: &[u8], neighbor_xforms: &[Vec<Mobius>; 2]) -> (Mobius, u8, bool) {
    let mut facing: u8 = 0;
    let mut transform = Mobius::identity();
    let mut parity = false;

    for &letter in word {
        match letter {
            A => {
                transform =
                    transform.compose(&neighbor_xforms[parity as usize][facing as usize]);
                parity = !parity;
                facing = (facing + 2) % 4;
            }
            B => {
                facing = (facing + 1) % 4;
            }
            B_INV => {
                facing = (facing + 3) % 4;
            }
            _ => unreachable!("invalid letter: {}", letter),
        }
    }

    (transform, facing, parity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::poincare::Complex;

    const EPS: f64 = 1e-10;

    fn cfg45() -> TilingConfig {
        TilingConfig::new(4, 5)
    }

    fn graph() -> CellGraph {
        CellGraph::new(&cfg45())
    }

    // --- word_to_mobius ---

    #[test]
    fn test_word_to_mobius_identity() {
        let g = graph();
        let m = word_to_mobius(&[], &g.neighbor_xforms);
        let z = m.apply(Complex::ZERO);
        assert!(z.abs() < EPS, "empty word should map to origin, got {:?}", z);
    }

    #[test]
    fn test_word_to_mobius_aa_identity() {
        let g = graph();
        let m = word_to_mobius(&[A, A], &g.neighbor_xforms);
        let z = m.apply(Complex::ZERO);
        assert!(z.abs() < EPS, "aa should map to origin, got {:?}", z);
    }

    #[test]
    fn test_word_to_mobius_ab5_identity() {
        let g = graph();
        let word: Vec<u8> = (0..5).flat_map(|_| [A, B]).collect();
        let m = word_to_mobius(&word, &g.neighbor_xforms);
        let z = m.apply(Complex::ZERO);
        assert!(
            z.abs() < 1e-6,
            "(aB)^5 should map to origin, got |z| = {}",
            z.abs()
        );
        assert!(
            m.b.abs() < 1e-6,
            "(aB)^5 Mobius b should be ~0, got |b| = {}",
            m.b.abs()
        );
    }

    #[test]
    fn test_word_to_mobius_b_is_noop() {
        // B only changes facing, not the transform.
        let g = graph();
        let m_a = word_to_mobius(&[A], &g.neighbor_xforms);
        let m_ab = word_to_mobius(&[A, B], &g.neighbor_xforms);
        let c_a = m_a.apply(Complex::ZERO);
        let c_ab = m_ab.apply(Complex::ZERO);
        assert!(
            (c_a - c_ab).abs() < EPS,
            "'a' and 'aB' should have same center"
        );
    }

    // --- BFS expansion ---

    #[test]
    fn test_expand_bfs_depth_0() {
        let g = graph();
        // Constructor already adds origin.
        assert_eq!(g.cells.len(), 1, "depth 0 should have just origin");
    }

    #[test]
    fn test_expand_bfs_depth_1() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 1);
        assert_eq!(g.cells.len(), 5, "depth 1: origin + 4 neighbors = 5");
    }

    #[test]
    fn test_expand_bfs_depth_2() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 2);
        assert_eq!(g.cells.len(), 17, "depth 2: expected 17 cells");
    }

    #[test]
    fn test_expand_bfs_depth_3() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 3);
        assert_eq!(g.cells.len(), 45, "depth 3: expected 45 cells");
    }

    #[test]
    fn test_no_duplicate_cell_ids() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 3);
        for (key, data) in &g.cells {
            assert_eq!(key, &data.id, "key mismatch for {:?}", key);
        }
    }

    // --- Neighbor validity ---

    #[test]
    fn test_all_neighbors_valid_depth_3() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 3);
        // Cells at depth <= 2 should have all neighbors loaded (since we expanded to 3).
        let inner = g.cells_within(&origin, 2);
        for cell_id in &inner {
            let data = &g.cells[*cell_id];
            for (e, neighbor_id) in data.neighbors.iter().enumerate() {
                assert!(
                    g.cells.contains_key(neighbor_id),
                    "cell {} edge {} neighbor {} not in graph",
                    cell_id, e, neighbor_id,
                );
            }
        }
    }

    #[test]
    fn test_neighbor_roundtrip_depth_3() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 3);

        for (cell_id, data) in &g.cells {
            for e in 0..4usize {
                let neighbor_id = &data.neighbors[e];
                if let Some(neighbor_data) = g.cells.get(neighbor_id) {
                    let back_edge = data.neighbor_orientations[e] as usize;
                    let back_id = &neighbor_data.neighbors[back_edge];
                    assert_eq!(
                        cell_id, back_id,
                        "roundtrip failed: {} --e{}--> {} --e{}--> {} (expected {})",
                        cell_id, e, neighbor_id, back_edge, back_id, cell_id,
                    );
                }
            }
        }
    }

    #[test]
    fn test_neighbor_symmetry() {
        // If A is neighbor of B across edge j, then B is neighbor of A across some edge k.
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 3);

        for (cell_id, data) in &g.cells {
            for e in 0..4usize {
                let neighbor_id = &data.neighbors[e];
                if let Some(neighbor_data) = g.cells.get(neighbor_id) {
                    let found = neighbor_data.neighbors.iter().any(|n| n == cell_id);
                    assert!(
                        found,
                        "cell {} is neighbor of {} but not vice versa",
                        cell_id, neighbor_id,
                    );
                }
            }
        }
    }

    // --- Mobius validation ---

    #[test]
    fn test_mobius_from_word_matches_stored() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 3);

        for (cell_id, data) in &g.cells {
            let word_mob = word_to_mobius(cell_id.word(), &g.neighbor_xforms);
            let word_center = word_mob.apply(Complex::ZERO);
            let stored_center = data.mobius.apply(Complex::ZERO);
            let dist = (word_center - stored_center).abs();
            assert!(
                dist < EPS,
                "Mobius mismatch for {}: dist = {:.2e}",
                cell_id, dist,
            );
        }
    }

    #[test]
    fn test_mobius_incremental_vs_word() {
        // Verify incremental BFS Mobius matches word_to_mobius for all cells.
        let cfg = cfg45();
        let xforms = neighbor_transforms(&cfg);
        let rules = rewrite::load_rules(5);

        // Manual BFS with incremental Mobius computation.
        let origin = CellId::origin();
        let mut incremental: HashMap<CellId, Mobius> = HashMap::new();
        incremental.insert(origin.clone(), Mobius::identity());

        let mut frontier: Vec<CellId> = vec![origin.clone()];
        let mut visited: HashSet<CellId> = HashSet::new();
        visited.insert(origin);

        for _depth in 0..3 {
            let mut next = Vec::new();
            for cell_id in &frontier {
                let parent_mob = incremental[cell_id];
                let (_, parent_facing, parent_parity) =
                    word_to_mobius_state(cell_id.word(), &xforms);

                for e in 0..4u8 {
                    let n = cell_id::neighbor(cell_id, e, &rules);
                    if visited.insert(n.id.clone()) {
                        let physical_edge = ((parent_facing + e) % 4) as usize;
                        let child_mob = parent_mob
                            .compose(&xforms[parent_parity as usize][physical_edge]);
                        incremental.insert(n.id.clone(), child_mob);
                        next.push(n.id);
                    }
                }
            }
            frontier = next;
        }

        // Compare incremental centers with word_to_mobius centers.
        for (cell_id, inc_mob) in &incremental {
            let word_mob = word_to_mobius(cell_id.word(), &xforms);
            let inc_center = inc_mob.apply(Complex::ZERO);
            let word_center = word_mob.apply(Complex::ZERO);
            let dist = (inc_center - word_center).abs();
            assert!(
                dist < EPS,
                "incremental vs word mismatch for {}: dist = {:.2e}",
                cell_id, dist,
            );
        }
    }

    #[test]
    fn test_mobius_centers_inside_disk() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 3);

        for (cell_id, data) in &g.cells {
            let center = data.mobius.apply(Complex::ZERO);
            assert!(
                center.abs() < 1.0,
                "cell {} center outside disk: |z| = {}",
                cell_id, center.abs(),
            );
        }
    }

    #[test]
    fn test_mobius_centers_distinct() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 3);

        let centers: Vec<(&CellId, Complex)> = g
            .cells
            .iter()
            .map(|(id, data)| (id, data.mobius.apply(Complex::ZERO)))
            .collect();

        for i in 0..centers.len() {
            for j in (i + 1)..centers.len() {
                let dist = (centers[i].1 - centers[j].1).abs();
                assert!(
                    dist > 1e-4,
                    "cells {} and {} have nearly identical centers: dist = {:.2e}",
                    centers[i].0, centers[j].0, dist,
                );
            }
        }
    }

    // --- cells_within ---

    #[test]
    fn test_cells_within_0() {
        let g = graph();
        let result = g.cells_within(&g.origin, 0);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_cells_within_1() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 1);
        let result = g.cells_within(&origin, 1);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_cells_within_subset() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 3);
        let r2: HashSet<&CellId> = g.cells_within(&origin, 2).into_iter().collect();
        let r3: HashSet<&CellId> = g.cells_within(&origin, 3).into_iter().collect();
        assert!(r2.is_subset(&r3));
        assert!(r3.len() > r2.len());
    }

    // --- ensure_neighborhood ---

    #[test]
    fn test_ensure_neighborhood_idempotent() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.ensure_neighborhood(&origin, 2);
        let count1 = g.cells.len();
        g.ensure_neighborhood(&origin, 2);
        let count2 = g.cells.len();
        assert_eq!(count1, count2, "ensure_neighborhood should be idempotent");
    }

    // --- Parity ---

    #[test]
    fn test_parity_origin_even() {
        let g = graph();
        assert!(!g.cells[&g.origin].parity, "origin should have even parity");
    }

    #[test]
    fn test_parity_alternates() {
        let mut g = graph();
        let origin = g.origin.clone();
        g.expand_bfs(&origin, 2);

        for neighbor_id in &g.cells[&origin].neighbors.clone() {
            assert!(
                g.cells[neighbor_id].parity,
                "origin neighbor {} should have odd parity",
                neighbor_id,
            );
            // Neighbor's neighbors should be even (or origin).
            for nn_id in &g.cells[neighbor_id].neighbors.clone() {
                let nn = &g.cells[nn_id];
                assert!(
                    !nn.parity,
                    "depth-2 cell {} should have even parity",
                    nn_id,
                );
            }
        }
    }
}
