//! Phase 4: Comprehensive algebraic tests for the Knuth-Bendix cell identity system.
//!
//! These tests validate the rewrite engine, CellId canonicalization, CellGraph,
//! and cross-validate against the old spatial TilingState system.

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::time::Instant;

    use crate::hyperbolic::cell_graph::CellGraph;
    use crate::hyperbolic::cell_id::{self, CellId};
    use crate::hyperbolic::poincare::{Complex, TilingConfig};
    use crate::hyperbolic::rewrite::{self, RewriteRule, Word, A, B, B_INV};
    use crate::hyperbolic::tiling::TilingState;

    fn rules() -> Vec<RewriteRule> {
        rewrite::load_rules(5)
    }

    fn cfg45() -> TilingConfig {
        TilingConfig::new(4, 5)
    }

    // ========================================================================
    // Long walk test
    // ========================================================================

    #[test]
    fn test_long_walk_100_steps_and_back() {
        // Walk 100 steps always crossing edge 0, recording the back-edge.
        // Then walk back 100 steps using those back-edges.
        // Final CellId must be origin.
        let r = rules();
        let mut current = CellId::origin();
        let mut back_edges: Vec<u8> = Vec::new();

        for step in 0..100 {
            let n = cell_id::neighbor(&current, 0, &r);
            // Find which edge of the neighbor leads back to current.
            let mut back_edge = None;
            for e in 0..4 {
                if cell_id::neighbor(&n.id, e, &r).id == current {
                    back_edge = Some(e);
                    break;
                }
            }
            let back_edge = back_edge.unwrap_or_else(|| {
                panic!("no back edge at step {step}: {} -> {}", current, n.id)
            });
            back_edges.push(back_edge);
            current = n.id;
        }

        // Walk back.
        for step in 0..100 {
            let back_edge = back_edges[99 - step];
            let n = cell_id::neighbor(&current, back_edge, &r);
            current = n.id;
        }

        assert!(
            current.is_empty(),
            "after 100 steps forward and 100 back, should be at origin, got {}",
            current,
        );
    }

    #[test]
    fn test_long_walk_each_direction() {
        // Walk 50 steps in each of the 4 edge directions, then retrace.
        let r = rules();

        for start_edge in 0..4u8 {
            let mut current = CellId::origin();
            let mut back_edges: Vec<u8> = Vec::new();

            for _ in 0..50 {
                let n = cell_id::neighbor(&current, start_edge, &r);
                let mut back_edge = None;
                for e in 0..4 {
                    if cell_id::neighbor(&n.id, e, &r).id == current {
                        back_edge = Some(e);
                        break;
                    }
                }
                back_edges.push(back_edge.unwrap());
                current = n.id;
            }

            for step in 0..50 {
                let back_edge = back_edges[49 - step];
                current = cell_id::neighbor(&current, back_edge, &r).id;
            }

            assert!(
                current.is_empty(),
                "edge {start_edge}: after 50 steps forward and 50 back, got {}",
                current,
            );
        }
    }

    // ========================================================================
    // Random walk test
    // ========================================================================

    #[test]
    fn test_random_walk_1000_steps() {
        // Deterministic "random" walk using a simple LCG.
        // At each step: pick an edge, compute neighbor.
        // Verify: CellId is valid (reduces to itself), no panics.
        let r = rules();
        let mut current = CellId::origin();
        let mut rng: u64 = 0xDEADBEEF; // seed

        for step in 0..1000 {
            // LCG: rng = (rng * 6364136223846793005 + 1) mod 2^64
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let edge = ((rng >> 33) % 4) as u8;

            let n = cell_id::neighbor(&current, edge, &r);

            // Verify the CellId is canonical (reduces to itself).
            let recanon = cell_id::canonicalize(n.id.word(), &r);
            assert_eq!(
                n.id, recanon.id,
                "step {step}: neighbor CellId is not canonical: {} vs {}",
                n.id, recanon.id,
            );

            // Verify all 4 neighbors exist and are consistent.
            let neighbors = cell_id::all_neighbors(&n.id, &r);
            let back_found = neighbors.iter().any(|nn| nn.id == current);
            assert!(
                back_found,
                "step {step}: neighbor {} has no back-edge to {}",
                n.id, current,
            );

            current = n.id;
        }
    }

    #[test]
    fn test_random_walk_return_probability() {
        // Walk 500 steps away (recording back-edges), then walk back.
        // Uses a deterministic "random" direction at each step.
        let r = rules();
        let mut current = CellId::origin();
        let mut back_edges: Vec<u8> = Vec::new();
        let mut rng: u64 = 0xCAFEBABE;

        for _ in 0..500 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let edge = ((rng >> 33) % 4) as u8;

            let n = cell_id::neighbor(&current, edge, &r);
            // Find back-edge.
            let mut back = 0u8;
            for e in 0..4 {
                if cell_id::neighbor(&n.id, e, &r).id == current {
                    back = e;
                    break;
                }
            }
            back_edges.push(back);
            current = n.id;
        }

        // Retrace.
        for step in 0..500 {
            let back_edge = back_edges[499 - step];
            current = cell_id::neighbor(&current, back_edge, &r).id;
        }

        assert!(
            current.is_empty(),
            "random walk 500 steps + retrace should return to origin, got {}",
            current,
        );
    }

    // ========================================================================
    // Cross-validation: algebraic CellGraph vs. spatial TilingState
    // ========================================================================

    #[test]
    fn test_cross_validation_depth_5() {
        // Expand both systems to depth 5 and verify 1:1 correspondence.
        let cfg = cfg45();

        // Algebraic system: BFS from CellGraph.
        let mut graph = CellGraph::new(&cfg);
        let origin = graph.origin.clone();
        graph.expand_bfs(&origin, 5);

        // Collect algebraic cell centers.
        let algebraic_centers: HashMap<&CellId, Complex> = graph
            .cells
            .iter()
            .map(|(id, data)| (id, data.mobius.apply(Complex::ZERO)))
            .collect();

        // Spatial system: expand TilingState.
        let mut tiling = TilingState::new(cfg);
        // Expand enough to cover depth 5.
        for _ in 0..20 {
            tiling.expand_near(Complex::ZERO, 10.0);
        }

        // Build a spatial lookup: for each algebraic cell, find the closest
        // spatial tile. We need to match them up.
        let spatial_centers: Vec<Complex> = tiling
            .tiles
            .iter()
            .map(|t| t.transform.apply(Complex::ZERO))
            .collect();

        // For each algebraic cell, find the nearest spatial tile.
        let mut matched_spatial: HashSet<usize> = HashSet::new();
        let mut unmatched_algebraic: Vec<&CellId> = Vec::new();

        for (cell_id, &alg_center) in &algebraic_centers {
            let mut best_idx = None;
            let mut best_dist = f64::MAX;
            for (i, &sc) in spatial_centers.iter().enumerate() {
                let dist = (alg_center - sc).abs();
                if dist < best_dist {
                    best_dist = dist;
                    best_idx = Some(i);
                }
            }
            if let Some(idx) = best_idx {
                if best_dist < 1e-6 {
                    matched_spatial.insert(idx);
                } else {
                    unmatched_algebraic.push(cell_id);
                }
            }
        }

        // Every algebraic cell should have a spatial match.
        assert!(
            unmatched_algebraic.is_empty(),
            "{} algebraic cells have no spatial match (e.g. {})",
            unmatched_algebraic.len(),
            unmatched_algebraic.first().map_or("?".to_string(), |c| c.to_string()),
        );

        // The number of matched spatial tiles should equal the algebraic count.
        // (Spatial may have more tiles since it doesn't respect exact BFS depth.)
        assert_eq!(
            matched_spatial.len(),
            algebraic_centers.len(),
            "matched spatial {} != algebraic {}",
            matched_spatial.len(),
            algebraic_centers.len(),
        );
    }

    // ========================================================================
    // Orientation consistency
    // ========================================================================

    #[test]
    fn test_orientation_consistency_depth_4() {
        // For every cell in BFS depth 4, verify that all 4 orientations
        // produce the same CellId.
        let r = rules();
        let mut graph = CellGraph::new(&cfg45());
        let origin = graph.origin.clone();
        graph.expand_bfs(&origin, 4);

        for (cell_id, _data) in &graph.cells {
            let word = cell_id.word();
            // Try all 4 orientations: word, word·B, word·BB, word·BBB.
            let oc0 = cell_id::canonicalize(word, &r);
            let mut w1 = word.to_vec();
            w1.push(B);
            let oc1 = cell_id::canonicalize(&w1, &r);
            let mut w2 = word.to_vec();
            w2.extend_from_slice(&[B, B]);
            let oc2 = cell_id::canonicalize(&w2, &r);
            let mut w3 = word.to_vec();
            w3.extend_from_slice(&[B, B, B]);
            let oc3 = cell_id::canonicalize(&w3, &r);

            assert_eq!(
                oc0.id, *cell_id,
                "cell {} orientation 0 mismatch: {}",
                cell_id, oc0.id,
            );
            assert_eq!(
                oc1.id, *cell_id,
                "cell {} orientation 1 mismatch: {}",
                cell_id, oc1.id,
            );
            assert_eq!(
                oc2.id, *cell_id,
                "cell {} orientation 2 mismatch: {}",
                cell_id, oc2.id,
            );
            assert_eq!(
                oc3.id, *cell_id,
                "cell {} orientation 3 mismatch: {}",
                cell_id, oc3.id,
            );

            // All 4 orientations should be distinct (0, 1, 2, 3).
            let orientations: HashSet<u8> =
                [oc0.orientation, oc1.orientation, oc2.orientation, oc3.orientation]
                    .into_iter()
                    .collect();
            assert_eq!(
                orientations.len(),
                4,
                "cell {}: expected 4 distinct orientations, got {:?}",
                cell_id,
                [oc0.orientation, oc1.orientation, oc2.orientation, oc3.orientation],
            );
        }
    }

    // ========================================================================
    // Neighbor symmetry (depth 4)
    // ========================================================================

    #[test]
    fn test_neighbor_symmetry_depth_4() {
        // If A is neighbor of B across edge j, then B is neighbor of A
        // across some edge k. Verify for ALL cells in BFS depth 4.
        let r = rules();
        let mut graph = CellGraph::new(&cfg45());
        let origin = graph.origin.clone();
        graph.expand_bfs(&origin, 4);

        // Only check cells up to depth 3 (so all neighbors are loaded at depth 4).
        let inner = graph.cells_within(&origin, 3);

        for cell_id in &inner {
            let data = &graph.cells[*cell_id];
            for e in 0..4usize {
                let neighbor_id = &data.neighbors[e];
                let back_edge = data.neighbor_orientations[e];

                if let Some(neighbor_data) = graph.cells.get(neighbor_id) {
                    // Verify the back-edge points back to us.
                    assert_eq!(
                        &neighbor_data.neighbors[back_edge as usize], *cell_id,
                        "symmetry broken: {} --e{}--> {} --e{}--> {} (expected {})",
                        cell_id, e, neighbor_id, back_edge,
                        neighbor_data.neighbors[back_edge as usize], cell_id,
                    );
                }

                // Also verify via recomputation.
                let computed = cell_id::neighbor(neighbor_id, back_edge, &r);
                assert_eq!(
                    &computed.id, *cell_id,
                    "recomputed back-edge mismatch: {} --e{}--> {} --e{}--> {} (expected {})",
                    cell_id, e, neighbor_id, back_edge, computed.id, cell_id,
                );
            }
        }
    }

    // ========================================================================
    // Growth rate
    // ========================================================================

    #[test]
    fn test_growth_rate_depth_0_to_7() {
        // Verify {4,5} cell counts at each BFS depth.
        // Known: depth 0→3 = 1, 5, 17, 45.
        // Exponential growth characteristic of hyperbolic tilings.
        let r = rules();
        let origin = CellId::origin();
        let mut cells: HashSet<CellId> = HashSet::new();
        let mut frontier: Vec<CellId> = vec![origin.clone()];
        cells.insert(origin);

        let mut counts: Vec<usize> = vec![1]; // depth 0

        for depth in 1..=7 {
            let mut next = Vec::new();
            for cell in &frontier {
                for n in cell_id::all_neighbors(cell, &r) {
                    if cells.insert(n.id.clone()) {
                        next.push(n.id);
                    }
                }
            }
            counts.push(cells.len());
            eprintln!("depth {depth}: {} cells (+{} new)", cells.len(), next.len());
            frontier = next;
        }

        // Verify known counts.
        assert_eq!(counts[0], 1, "depth 0");
        assert_eq!(counts[1], 5, "depth 1");
        assert_eq!(counts[2], 17, "depth 2");
        assert_eq!(counts[3], 45, "depth 3");

        // Verify exponential growth: each layer should add more cells than the previous.
        for i in 2..counts.len() {
            let growth_prev = counts[i] - counts[i - 1];
            let growth_before = counts[i - 1] - counts[i - 2];
            assert!(
                growth_prev >= growth_before,
                "growth should be non-decreasing: depth {} added {}, depth {} added {}",
                i, growth_prev, i - 1, growth_before,
            );
        }

        // Sanity: depth 7 should have a reasonable number of cells.
        assert!(
            counts[7] > 500,
            "depth 7 should have >500 cells, got {}",
            counts[7],
        );
    }

    // ========================================================================
    // Benchmarks
    // ========================================================================

    /// Build a word by doing a random walk of `n` turtle moves.
    fn build_random_word(n: usize, seed: u64) -> Word {
        let mut rng = seed;
        let mut w = Vec::with_capacity(n * 2);
        for _ in 0..n {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let choice = (rng >> 33) % 3;
            match choice {
                0 => w.push(A),
                1 => w.push(B),
                2 => w.push(B_INV),
                _ => unreachable!(),
            }
        }
        w
    }

    #[test]
    fn bench_reduce_various_lengths() {
        let r = rules();
        let lengths = [10, 50, 100, 500];

        for &len in &lengths {
            let word = build_random_word(len, 42 + len as u64);
            let iterations = 1000;

            let start = Instant::now();
            for _ in 0..iterations {
                let mut w = word.clone();
                rewrite::reduce(&mut w, &r);
            }
            let elapsed = start.elapsed();
            let per_iter = elapsed / iterations;

            eprintln!(
                "reduce(len={len}): {per_iter:?}/iter ({} total)",
                humanize(elapsed),
            );

            // Words of typical rendering distance (< 50) should take < 1ms.
            if len <= 50 {
                assert!(
                    per_iter.as_micros() < 1000,
                    "reduce(len={len}) took {:?}, expected < 1ms",
                    per_iter,
                );
            }
        }
    }

    #[test]
    fn bench_canonicalize_various_lengths() {
        let r = rules();
        let lengths = [10, 50, 100, 500];

        for &len in &lengths {
            let word = build_random_word(len, 123 + len as u64);
            let iterations = 1000;

            let start = Instant::now();
            for _ in 0..iterations {
                let _ = cell_id::canonicalize(&word, &r);
            }
            let elapsed = start.elapsed();
            let per_iter = elapsed / iterations;

            eprintln!(
                "canonicalize(len={len}): {per_iter:?}/iter ({} total)",
                humanize(elapsed),
            );

            if len <= 50 {
                assert!(
                    per_iter.as_micros() < 1000,
                    "canonicalize(len={len}) took {:?}, expected < 1ms",
                    per_iter,
                );
            }
        }
    }

    #[test]
    fn bench_bfs_expansion() {
        let cfg = cfg45();
        let depths = [5, 8, 10];

        for &depth in &depths {
            let start = Instant::now();
            let mut graph = CellGraph::new(&cfg);
            let origin = graph.origin.clone();
            graph.expand_bfs(&origin, depth);
            let elapsed = start.elapsed();

            eprintln!(
                "BFS depth {depth}: {} cells in {} ({:.1} cells/ms)",
                graph.cells.len(),
                humanize(elapsed),
                graph.cells.len() as f64 / elapsed.as_secs_f64() / 1000.0,
            );
        }
    }

    #[test]
    fn bench_reduce_typical_words_under_1ms() {
        // The plan requires: "Ensure reduction of typical words (length < 50) takes < 1ms"
        let r = rules();

        // Build 100 different typical words and reduce each.
        for seed in 0..100u64 {
            let word = build_random_word(40, seed);
            let start = Instant::now();
            let mut w = word.clone();
            rewrite::reduce(&mut w, &r);
            let elapsed = start.elapsed();

            assert!(
                elapsed.as_micros() < 1000,
                "reduce(seed={seed}, input_len={}, output_len={}) took {:?}",
                word.len(),
                w.len(),
                elapsed,
            );
        }
    }

    fn humanize(d: std::time::Duration) -> String {
        if d.as_secs() > 0 {
            format!("{:.2}s", d.as_secs_f64())
        } else if d.as_millis() > 0 {
            format!("{}ms", d.as_millis())
        } else {
            format!("{}us", d.as_micros())
        }
    }
}
