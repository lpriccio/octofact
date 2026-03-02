//! Algebraic cell identity for {4,5} hyperbolic tiling.
//!
//! A `CellId` is the canonical (shortlex-minimum) reduced word representing a cell,
//! chosen from the 4 orientations of that cell. An `OrientedCell` pairs a CellId
//! with an orientation (0–3) tracking which edge the turtle faces.


use std::fmt;
use std::hash::{Hash, Hasher};

use super::rewrite::{self, RewriteRule, Word, A, B};

/// Canonical cell identity: the shortlex-minimum reduced word among
/// the 4 orientations of a cell.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct CellId {
    /// The canonical word (shortlex minimum of the 4 orientations).
    word: Word,
}

impl CellId {
    /// The origin cell (empty word).
    pub fn origin() -> Self {
        Self { word: vec![] }
    }

    /// Create a CellId from an already-canonical word. No validation.
    #[allow(dead_code)]
    pub fn from_canonical(word: Word) -> Self {
        Self { word }
    }

    /// The canonical word.
    pub fn word(&self) -> &[u8] {
        &self.word
    }

    /// Word length (a rough measure of distance from origin).
    pub fn len(&self) -> usize {
        self.word.len()
    }

    pub fn is_empty(&self) -> bool {
        self.word.is_empty()
    }

    /// Number of hops (A generators) in the word, ignoring turns.
    pub fn hop_count(&self) -> usize {
        self.word.iter().filter(|&&g| g == A).count()
    }
}

impl Hash for CellId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.word.hash(state);
    }
}

impl fmt::Debug for CellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CellId({})", rewrite::word_to_string(&self.word))
    }
}

impl fmt::Display for CellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", rewrite::word_to_string(&self.word))
    }
}

/// A cell with a specific orientation (which edge the turtle faces).
#[derive(Clone, Debug)]
pub struct OrientedCell {
    pub id: CellId,
    /// 0–3: how many B (left turns) from the canonical orientation to reach
    /// the orientation of the word that produced this cell.
    #[allow(dead_code)]
    pub orientation: u8,
}

/// Reduce a word and canonicalize: try all 4 orientations, pick shortlex minimum.
///
/// Returns the CellId and the orientation offset (how many B's were appended
/// to the original word to produce the canonical form).
pub fn canonicalize(word: &[u8], rules: &[RewriteRule]) -> OrientedCell {
    // Reduce the base word.
    let mut base = word.to_vec();
    rewrite::reduce(&mut base, rules);

    let mut best = base.clone();
    let mut best_rot: u8 = 0;

    // Try appending 1, 2, 3 B's (left turns) and reducing.
    let mut rotated = base;
    for rot in 1..4u8 {
        rotated.push(B);
        let mut candidate = rotated.clone();
        rewrite::reduce(&mut candidate, rules);
        if rewrite::shortlex_cmp(&candidate, &best) == std::cmp::Ordering::Less {
            best = candidate;
            best_rot = rot;
        }
    }

    OrientedCell {
        id: CellId { word: best },
        orientation: best_rot,
    }
}

/// Compute the neighbor of a cell across a given edge.
///
/// `orientation` is the orientation of the starting cell (from canonicalization).
/// `edge` is the edge index (0–3) in the CANONICAL frame of the cell.
///
/// The turtle at the canonical cell faces edge 0. To cross edge `edge`:
/// 1. Turn to face that edge: append B^edge
/// 2. Cross: append 'a'
/// 3. Reduce and canonicalize.
///
/// The returned OrientedCell gives the neighbor's CellId and the orientation
/// that tells which edge of the neighbor connects back.
pub fn neighbor(cell: &CellId, edge: u8, rules: &[RewriteRule]) -> OrientedCell {
    let mut word = cell.word.clone();
    // Turn to face the target edge (from canonical orientation 0).
    word.extend(std::iter::repeat_n(B, edge as usize));
    // Cross the edge.
    word.push(A);
    canonicalize(&word, rules)
}

/// Compute all 4 neighbors of a cell (one per edge in canonical orientation).
/// Returns [(neighbor_id, orientation); 4] for edges 0, 1, 2, 3.
pub fn all_neighbors(cell: &CellId, rules: &[RewriteRule]) -> [OrientedCell; 4] {
    [
        neighbor(cell, 0, rules),
        neighbor(cell, 1, rules),
        neighbor(cell, 2, rules),
        neighbor(cell, 3, rules),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn rules() -> Vec<RewriteRule> {
        rewrite::load_rules(5)
    }

    // --- Basic canonicalization ---

    #[test]
    fn test_origin_canonical() {
        let oc = canonicalize(&[], &rules());
        assert!(oc.id.is_empty(), "origin should be empty word, got {:?}", oc.id);
    }

    #[test]
    fn test_origin_rotations_same_cell() {
        let r = rules();
        // B, BB, BBB all represent the origin with different orientations.
        let oc0 = canonicalize(&[], &r);
        let oc1 = canonicalize(&[B], &r);
        let oc2 = canonicalize(&[B, B], &r);
        let oc3 = canonicalize(&[B, B, B], &r);
        assert_eq!(oc0.id, oc1.id);
        assert_eq!(oc0.id, oc2.id);
        assert_eq!(oc0.id, oc3.id);
        // Orientations should differ.
        let orientations: HashSet<u8> = [oc0.orientation, oc1.orientation, oc2.orientation, oc3.orientation]
            .into_iter()
            .collect();
        assert_eq!(orientations.len(), 4, "4 distinct orientations expected");
    }

    #[test]
    fn test_single_a_canonical() {
        // 'a' = cross edge 0. This is a different cell from origin.
        let oc = canonicalize(&[A], &rules());
        assert!(!oc.id.is_empty(), "'a' should not be origin");
    }

    #[test]
    fn test_aa_is_origin() {
        // aa = identity → origin.
        let oc = canonicalize(&[A, A], &rules());
        assert!(oc.id.is_empty(), "aa should be origin, got {:?}", oc.id);
    }

    #[test]
    fn test_four_orientations_of_neighbor() {
        let r = rules();
        // 'a', 'aB', 'aBB', 'aBBB' are the same cell (neighbor across edge 0)
        // with different orientations.
        let oc_a = canonicalize(&rewrite::string_to_word("a"), &r);
        let oc_a_b = canonicalize(&rewrite::string_to_word("aB"), &r);
        let oc_a_bb = canonicalize(&rewrite::string_to_word("aBB"), &r);
        let oc_a_bbb = canonicalize(&rewrite::string_to_word("aBBB"), &r);
        assert_eq!(oc_a.id, oc_a_b.id, "a and aB should be same cell");
        assert_eq!(oc_a.id, oc_a_bb.id, "a and aBB should be same cell");
        assert_eq!(oc_a.id, oc_a_bbb.id, "a and aBBB should be same cell");
    }

    #[test]
    fn test_different_edges_different_cells() {
        let r = rules();
        // Crossing different edges should give different cells.
        let edge0 = canonicalize(&rewrite::string_to_word("a"), &r);
        let edge1 = canonicalize(&rewrite::string_to_word("Ba"), &r);
        let edge2 = canonicalize(&rewrite::string_to_word("bba"), &r); // BB→bb, so BBa = bba
        let edge3 = canonicalize(&rewrite::string_to_word("ba"), &r);
        assert_ne!(edge0.id, edge1.id, "edge 0 and edge 1 should differ");
        assert_ne!(edge0.id, edge2.id, "edge 0 and edge 2 should differ");
        assert_ne!(edge0.id, edge3.id, "edge 0 and edge 3 should differ");
        assert_ne!(edge1.id, edge2.id, "edge 1 and edge 2 should differ");
    }

    // --- Neighbor computation ---

    #[test]
    fn test_neighbor_roundtrip() {
        let r = rules();
        let origin = CellId::origin();
        // Neighbor across edge 0, then neighbor of that across the connecting edge.
        let n = neighbor(&origin, 0, &r);
        // The connecting edge in the neighbor depends on orientation.
        // But crossing edge 0 and then crossing back should give origin.
        // The word is: a (cross edge 0), then from the new cell,
        // to get back we need to cross the edge that connects back.
        // In the new cell's canonical frame, we need to find which edge leads back.
        // Rather than computing that, verify: for each edge of the neighbor,
        // check if crossing it returns to origin.
        let mut found_back = false;
        for edge in 0..4 {
            let nn = neighbor(&n.id, edge, &r);
            if nn.id == origin {
                found_back = true;
                break;
            }
        }
        assert!(found_back, "one of neighbor's edges should lead back to origin");
    }

    #[test]
    fn test_all_neighbors_of_origin() {
        let r = rules();
        let origin = CellId::origin();
        let neighbors = all_neighbors(&origin, &r);

        // 4 distinct non-origin neighbors.
        let ids: HashSet<&CellId> = neighbors.iter().map(|n| &n.id).collect();
        assert_eq!(ids.len(), 4, "origin should have 4 distinct neighbors");
        for n in &neighbors {
            assert_ne!(n.id, origin, "neighbor should not be origin");
        }
    }

    #[test]
    fn test_neighbor_symmetry() {
        // If A is neighbor of B, then B is neighbor of A.
        let r = rules();
        let origin = CellId::origin();
        let neighbors = all_neighbors(&origin, &r);
        for (edge, n) in neighbors.iter().enumerate() {
            let back_neighbors = all_neighbors(&n.id, &r);
            let found = back_neighbors.iter().any(|nn| nn.id == origin);
            assert!(
                found,
                "neighbor across edge {} ({:?}) should have origin as a neighbor",
                edge, n.id
            );
        }
    }

    #[test]
    fn test_vertex_cycle_5_cells() {
        // Walking around a vertex should return to origin after 5 steps.
        // At vertex between edges 0 and 1: cross edge 0, then in the new cell
        // cross the "next" edge repeatedly. With the turtle model:
        // (aB)^5 = e, so starting at origin, doing aB five times returns.
        // In neighbor terms: this means repeatedly taking neighbor edge 0,
        // but from the *oriented* perspective.
        //
        // Alternative check: just verify (aB)^5 = e at the word level.
        let r = rules();
        let word = rewrite::string_to_word("aBaBaBaBaB");
        let reduced = rewrite::reduced(&word, &r);
        assert!(reduced.is_empty(), "(aB)^5 should be identity, got {:?}",
            rewrite::word_to_string(&reduced));
    }

    // --- BFS growth ---

    #[test]
    fn test_bfs_depth_1() {
        let r = rules();
        let origin = CellId::origin();
        let mut cells: HashSet<CellId> = HashSet::new();
        cells.insert(origin.clone());

        let neighbors = all_neighbors(&origin, &r);
        for n in &neighbors {
            cells.insert(n.id.clone());
        }
        assert_eq!(cells.len(), 5, "depth 1: origin + 4 neighbors = 5");
    }

    #[test]
    fn test_bfs_depth_2() {
        let r = rules();
        let origin = CellId::origin();
        let mut cells: HashSet<CellId> = HashSet::new();
        let mut frontier: Vec<CellId> = vec![origin.clone()];
        cells.insert(origin);

        // Depth 1
        let mut next_frontier = vec![];
        for cell in &frontier {
            for n in all_neighbors(cell, &r) {
                if cells.insert(n.id.clone()) {
                    next_frontier.push(n.id);
                }
            }
        }
        frontier = next_frontier;

        // Depth 2
        let mut next_frontier2 = vec![];
        for cell in &frontier {
            for n in all_neighbors(cell, &r) {
                if cells.insert(n.id.clone()) {
                    next_frontier2.push(n.id);
                }
            }
        }
        let _frontier = next_frontier2;

        // {4,5} growth: each cell has 4 neighbors, 5 meet at each vertex.
        // Depth 0: 1, Depth 1: 5, Depth 2: should be more.
        // Exact count depends on the tiling — just sanity check.
        assert!(
            cells.len() > 5,
            "depth 2 should have more than 5 cells, got {}",
            cells.len()
        );
        assert!(
            cells.len() < 100,
            "depth 2 shouldn't have too many cells, got {}",
            cells.len()
        );
        eprintln!("BFS depth 2 cell count: {}", cells.len());
    }

    #[test]
    fn test_bfs_depth_3_no_duplicates() {
        let r = rules();
        let origin = CellId::origin();
        let mut cells: HashSet<CellId> = HashSet::new();
        let mut frontier: Vec<CellId> = vec![origin.clone()];
        cells.insert(origin);

        for _depth in 0..3 {
            let mut next = vec![];
            for cell in &frontier {
                for n in all_neighbors(cell, &r) {
                    if cells.insert(n.id.clone()) {
                        next.push(n.id);
                    }
                }
            }
            frontier = next;
        }
        // Just verify we got a reasonable count and no panics.
        eprintln!("BFS depth 3 cell count: {}", cells.len());
        assert!(cells.len() > 20, "too few cells at depth 3: {}", cells.len());
    }

    #[test]
    fn test_two_paths_same_cell() {
        // Go east then north, vs north then east. In hyperbolic space these
        // reach different cells (unlike Euclidean), but the algebraic system
        // should consistently identify them.
        let r = rules();
        // Path 1: cross edge 0, then from that cell cross edge 1.
        // Word: a (cross edge 0) + B (turn to edge 1) + a (cross) = aBa
        let path1 = canonicalize(&rewrite::string_to_word("aBa"), &r);
        // Path 2: cross edge 1, then from that cell cross edge 0.
        // Word: B (turn to edge 1) + a (cross) + b (turn back to edge 0... but
        // we need to think about what facing we have after crossing)
        // After Ba, we're in the edge-1 neighbor facing edge 2+1=3? No...
        // Actually "Ba" means: turn left then cross. After crossing, facing flips by +2.
        // So facing = (1+2)%4 = 3. To cross edge 0 from there, turn to 0: need (0-3+4)%4 = 1 B.
        // Word: BaBa
        let path2 = canonicalize(&rewrite::string_to_word("BaBa"), &r);
        // These should be DIFFERENT cells in hyperbolic space.
        // (In Euclidean, east-then-north = north-then-east. Not so in hyperbolic.)
        // Just verify both are well-defined and not origin.
        assert!(!path1.id.is_empty());
        assert!(!path2.id.is_empty());
        // They should indeed be different:
        assert_ne!(path1.id, path2.id, "east+north and north+east should be different cells in {{4,5}}");
    }
}
