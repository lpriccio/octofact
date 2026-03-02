# Knuth-Bendix Cell Identity Refactor

> Replace floating-point spatial deduplication with algebraically exact cell
> identity using the Knuth-Bendix completion algorithm. This eliminates
> floating-point drift when exploring far from origin and returning.

## Problem Statement

The current system identifies cells by their Poincare disk projection
coordinates, discretized to a 1e4 grid (`spatial_key`). Two paths to the same
cell are deduplicated by checking if their Mobius-transformed centers land in
the same grid bucket. This works near the origin but breaks down at large
distances due to floating-point drift in accumulated Mobius compositions.

The `reduce_address` function handles only two reduction rules (inverse
cancellation and vertex-cycle cancellation). These are incomplete — the full
{4,n} group has infinitely many relations, and the current reducer misses
non-trivial equivalences at higher depths.

## Solution: Algebraic Cell Identity

Use the Knuth-Bendix completion algorithm to build a convergent rewriting
system for the orientation-preserving symmetry group of the {4,n} tiling:

```
G = < A, a, B | Aa = aA = e, B^4 = e, (AB)^n = e >
```

where:
- **A** = step forward (cross the edge you're facing) — infinite order
- **a** = A⁻¹ = step backward
- **B** = turn left 90° — order 4

A word in {A, a, B} encodes a turtle-graphics path: a position AND orientation.
The rewriting system reduces any word to a unique **shortlex-minimal** canonical
form. Two words reduce to the same form iff they represent the same group
element (same position + same orientation).

### Cell vs. State

A **state** is a position + orientation (a group element / reduced word).
A **cell** is a position only — four states per cell (the 4 orientations).

For a state with reduced word `w`:
- The 4 orientations are: `reduce(w)`, `reduce(wB)`, `reduce(wBB)`, `reduce(wBBB)`
- **Canonical cell ID** = shortlex minimum of those 4 words
- **Orientation** = `r` where `reduce(w · B^r)` = canonical cell ID

### Neighbor Computation

From cell `C` with canonical word `w_C`, the neighbor across edge `k` (0–3,
where 0=forward from canonical orientation, 1=left, 2=behind, 3=right):

```
neighbor_state = reduce(w_C · B^k · A)
neighbor_cell  = canonicalize(neighbor_state)  →  (neighbor_id, neighbor_orientation)
connecting_edge_on_neighbor = (2 + k + neighbor_orientation) mod 4   [needs verification]
```

This is **exact** — no floating-point, no spatial hashing, no drift.

### Reference Implementation

`extern/knuth-bendix.py` contains a working Python implementation with tests
covering {4,5}, {4,6}, and {4,7}. It validates: defining relations, idempotency,
inverse cancellation, and identity insertion (500 random trials per order).

---

## Phase 1: Port Knuth-Bendix to Rust

> Pure algebraic module. No game dependencies. Thoroughly tested before
> proceeding.

**File**: `src/hyperbolic/knuth_bendix.rs`

**Data structures**:
- `type Letter = u8` with constants `FWD = b'A'`, `BACK = b'a'`, `LEFT = b'B'`
- `type Word = Vec<u8>` (ASCII bytes — readable in debug, efficient for matching)
- `struct RewriteSystem { rules: Vec<(Word, Word)>, vertex_order: u32 }`

**Core algorithms** (direct port from Python):
- [ ] `word_lt_shortlex(u, v) -> bool` — shortlex comparison
- [ ] `invert_word(w) -> Word` — reverse + swap A↔a, B→BBB
- [ ] `RewriteSystem::new(vertex_order, max_length)` — Knuth-Bendix completion
  - Seed rules from group presentation: Aa→ε, aA→ε, BBBB→ε, (AB)^n→ε, (BA)^n→ε
  - Orient each rule: shortlex-greater LHS → smaller RHS
  - Iterate: find critical pairs from rule overlaps, reduce both sides, add new
    rules when they differ, interreduce
  - Terminate when no new rules within max_length bound are generated
- [ ] `reduce(word) -> Word` — apply rules left-to-right until fixed point
- [ ] `interreduce(rules)` — clean up redundant rules

**Tests** (mirror Python test suite + extras):
- [ ] Defining relations: Aa=ε, aA=ε, B⁴=ε, (AB)^n=ε, (BA)^n=ε, (AB)^{2n}=ε
- [ ] Idempotency: reduce(reduce(w)) == reduce(w) for 1000 random words
- [ ] Inverse: reduce(w · invert(w)) == ε for 1000 random words
- [ ] Identity insertion: inserting a relator at any position doesn't change result
- [ ] Growth rate: count distinct elements by word length (compare to Python output)
- [ ] Test for n=5, n=6, n=7 (at minimum)
- [ ] Benchmark: reduction of 10K random words, completion time for various max_length

**Performance notes**:
- Completion runs once at startup (~100ms for max_length=40 in Python; Rust
  should be 10-50x faster)
- Reduction is hot path — called per cell discovery and neighbor lookup
- Consider: pre-sort rules by LHS length for early termination, or build an
  Aho-Corasick automaton for multi-pattern matching (optimization, not required
  for Phase 1)

**Milestone**: all Python tests pass in Rust. `cargo test` green. Print rule
counts and growth rates matching Python output.

---

## Phase 2: Canonical Cell Identity

> Build the CellId abstraction on top of the rewriter. This is the keystone
> data type for the entire refactor.

**File**: `src/hyperbolic/cell_id.rs`

**Data structures**:
```rust
/// Canonical cell identity — shortlex-minimum word over all 4 orientations.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellId(pub Word);

/// A cell with a specific orientation (which edge the "turtle" faces).
#[derive(Clone, Debug)]
pub struct OrientedCell {
    pub cell: CellId,
    pub rotation: u8,  // 0..3 — how many left turns from input state to canonical
}

/// An edge of a cell, identified by cell + edge index (0..3).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CellEdge {
    pub cell: CellId,
    pub edge: u8,  // 0=fwd from canonical orientation, 1=left, 2=behind, 3=right
}
```

**Core functions**:
- [ ] `canonicalize(rewriter, word) -> OrientedCell`
  - Reduce w, wB, wBB, wBBB; return shortlex min as CellId + rotation
- [ ] `neighbor(rewriter, cell, edge) -> (CellId, u8)`
  - Compute neighbor cell and which edge of the neighbor we connect to
  - `state = reduce(cell.0 · B^edge · A)`
  - Canonicalize state → (neighbor_cell, neighbor_rotation)
  - Connecting edge on neighbor = `(2 - neighbor_rotation + edge) mod 4`
    **[derive and verify this formula carefully with worked examples]**
- [ ] `all_neighbors(rewriter, cell) -> [(CellId, u8); 4]`
  - All 4 neighbors with connecting edges
- [ ] `distance_bound(cell) -> usize`
  - Rough distance estimate from origin (word length is an upper bound on
    hop count, since the canonical form is shortlex-minimal)
- [ ] `cells_within_radius(rewriter, center, radius) -> Vec<CellId>`
  - BFS from center cell up to `radius` hops
  - Returns all cells within that algebraic distance

**Critical tests**:
- [ ] Origin cell: canonicalize(ε) == canonicalize(B) == canonicalize(BB) == canonicalize(BBB)
- [ ] All 4 orientations of any cell yield the same CellId
- [ ] Neighbor symmetry: if X is neighbor of Y across edge e₁/e₂, then Y is
  neighbor of X across those same paired edges
- [ ] Walk-and-return: from any cell, walk N steps in direction d then N steps
  in opposite direction → back to same CellId
- [ ] Vertex cycle: walking around a vertex (n cells) returns to the start cell
- [ ] No duplicate CellIds in BFS expansion
- [ ] Neighbor edge formula: verify worked examples for {4,5} by hand
  (e.g., origin's 4 neighbors, and their neighbors)
- [ ] Compare neighbor graph structure to the Mobius-transform-based tiling
  for small tilings (first ~50 cells)

**Milestone**: CellId system passes all tests. BFS produces correct adjacency
graph matching the existing spatial-dedup tiling for the first ~100 cells.

---

## Phase 3: Neighbor Ring and Cell Registry

> Build the data structure that tracks which cells are loaded, their neighbors,
> and which cells are within rendering range.

**File**: `src/hyperbolic/cell_registry.rs`

**Data structures**:
```rust
pub struct CellRegistry {
    rewriter: RewriteSystem,
    /// All known cells, keyed by canonical ID.
    cells: HashMap<CellId, CellInfo>,
    /// For rendering: cells within ~3 hops of the camera cell.
    render_set: HashSet<CellId>,
    /// For simulation: all cells that have game state (always loaded).
    sim_set: HashSet<CellId>,
}

pub struct CellInfo {
    pub id: CellId,
    /// Immediate neighbors (always populated for loaded cells).
    pub neighbors: [CellId; 4],
    /// Which edge of each neighbor connects back to us.
    pub neighbor_edges: [u8; 4],
    /// Mobius transform (if within render range; computed on demand).
    pub transform: Option<Mobius>,
    /// BFS depth from camera (updated on camera move).
    pub render_depth: Option<usize>,
}
```

**Functions**:
- [ ] `CellRegistry::new(vertex_order)` — create rewriter, register origin cell
- [ ] `ensure_cell(cell_id) -> &CellInfo` — lazily create cell, compute neighbors
- [ ] `ensure_render_ring(center_cell, radius)` — BFS from center, populate
  render_set, compute Mobius transforms for all cells in ring
- [ ] `compute_transform(cell_id, base_cell, base_transform) -> Mobius`
  - Compute Mobius transform for a cell relative to a base cell
  - Walk the shortest path (BFS) from base to target, composing neighbor
    transforms at each step
  - This keeps transforms small (relative to camera, not absolute from origin)
- [ ] Mark cells with game state as persistent (never evicted from sim_set)

**Integration with existing Mobius transforms**:
- Neighbor transforms (the 4 geometric translations for crossing each edge)
  remain needed to convert algebraic adjacency into rendering geometry
- The key change: transforms are computed **relative to the camera cell**,
  not accumulated from origin. This keeps coefficients bounded regardless
  of exploration distance.
- `neighbor_xforms` from `poincare.rs` still provides the geometric step
  for each edge direction

**Tests**:
- [ ] Render ring at radius 3 from origin matches the existing BFS tile set
- [ ] All cells in render ring have valid transforms
- [ ] Moving camera to adjacent cell and rebuilding ring: transforms stay O(1)
- [ ] Cells with game state persist across camera moves

**Milestone**: CellRegistry can build and maintain a render ring around any cell,
with correct Mobius transforms. Ring structure matches existing tiling output.

---

## Phase 4: Integration — Replace TilingState

> Swap the spatial-dedup TilingState for the algebraic CellRegistry.
> This is the highest-risk phase; take it in small steps with regression tests.

### 4a: Parallel infrastructure

- [ ] Add `CellRegistry` alongside existing `TilingState` (don't remove yet)
- [ ] Wire `CellRegistry` into `RenderEngine` as an alternative path
- [ ] Add a feature flag or runtime toggle to switch between old and new systems
- [ ] Write comparison tests: for each cell in TilingState, verify corresponding
  CellId exists in CellRegistry with matching neighbors

### 4b: Replace tile identity

- [ ] Change `TileAddr` type alias from `SmallVec<[u8; 12]>` to `CellId`
- [ ] Update `Tile` struct: address field becomes `CellId`
- [ ] Update `WorldState`: all `HashMap<TileAddr, ...>` become `HashMap<CellId, ...>`
- [ ] Update `BeltNetwork`: cross-tile connections use `CellId`
- [ ] Update `MachinePool`, `InserterPool`, `PowerNetwork`: tile references → `CellId`
- [ ] Update `GridPos` in `world.rs` to use `CellId`

### 4c: Replace neighbor lookup

- [ ] `neighbor_tile_addr()` → delegates to `CellRegistry::neighbor()`
- [ ] Camera movement (`process_movement` in `camera.rs`):
  - When camera crosses tile boundary, compute new tile algebraically
  - Replace `find_tile_near(neighbor_center)` with algebraic neighbor lookup
  - Keep Mobius transforms for the actual camera position (continuous movement)
- [ ] Remove `spatial_key()`, `seen`, `spatial_to_tile` from TilingState

### 4d: Replace recentering

- [ ] Recentering becomes: update the "camera cell" in the registry
- [ ] Recompute render ring transforms relative to new camera cell
- [ ] No more eviction by disk radius — eviction is by algebraic hop distance
- [ ] Mobius transforms for render ring are always fresh (computed from short paths)

### 4e: Clean up

- [ ] Remove old `TilingState` (or gut it to a thin wrapper around CellRegistry)
- [ ] Remove `reduce_address()` (replaced by Knuth-Bendix)
- [ ] Remove `find_tile_near()`, `spatial_key()`
- [ ] Remove `parity` field from Tile (orientation handled by CellId system)
- [ ] Update all tests

**Milestone**: Game runs entirely on CellRegistry. Explore 100+ cells from
origin, return to origin, verify cell identity is preserved exactly. No NaN,
no misidentified cells.

---

## Phase 5: Performance Optimization

> Only after correctness is proven. Profile before optimizing.

- [ ] **Reduction hot path**: profile `reduce()` — if it's a bottleneck, consider:
  - Aho-Corasick multi-pattern matcher for rule LHS matching
  - Trie-based rule lookup
  - Memoization cache for recently reduced words (LRU, keyed by input word)
- [ ] **Word allocation**: consider `SmallVec<[u8; 32]>` or arena allocation
  for Word to avoid heap allocs on short canonical forms
- [ ] **Startup time**: completion for max_length=40 should be <100ms; if not,
  consider serializing the completed rule set and loading from cache
- [ ] **Render ring update**: incremental update when camera moves one cell
  (add new fringe cells, drop distant ones) rather than full BFS rebuild
- [ ] **Neighbor transform cache**: memoize `compute_transform()` results for
  cells that haven't moved relative to camera

---

## Mapping: Old System → New System

| Old concept | New concept |
|---|---|
| `TileAddr = SmallVec<[u8; 12]>` | `CellId(Word)` — shortlex-canonical word |
| `reduce_address(addr, p, q)` | `rewriter.reduce(word)` + `canonicalize()` |
| `spatial_key(center)` | Not needed — identity is algebraic |
| `seen: HashSet<(i64, i64)>` | `cells: HashMap<CellId, CellInfo>` |
| `spatial_to_tile: HashMap<(i64, i64), usize>` | `cells: HashMap<CellId, CellInfo>` |
| `find_tile_near(center)` | `registry.neighbor(cell, edge)` |
| `neighbor_tile_addr(idx, edge)` | `registry.neighbor(cell, edge).0` |
| `recenter_on(center_idx)` | `registry.ensure_render_ring(camera_cell, 3)` |
| Direction indices 0,1,2,3 | Edges 0,1,2,3 of canonical cell orientation |
| `parity: bool` | Subsumed by canonical orientation tracking |
| `depth: usize` | `word.len()` (canonical word length ≈ algebraic distance) |

---

## Key Formulas to Derive and Verify

These formulas need careful hand-worked examples before implementation:

1. **Connecting edge formula**: When cell C's edge k leads to neighbor N,
   which edge of N connects back to C?
   - Work out for origin→neighbor in all 4 directions in {4,5}
   - Verify the formula generalizes

2. **Direction mapping**: Relationship between old direction indices (0–3) and
   new edge indices (0–3 relative to canonical orientation)
   - The old system's direction `d` from tile with orientation `r` corresponds
     to edge `(d + r) mod 4`? Or `(d - r) mod 4`? Derive carefully.

3. **Camera orientation**: When the camera crosses from cell C (edge k) into
   neighbor N (edge k'), what is the camera's new orientation relative to N's
   canonical orientation?

---

## Notes for Continuation Across Context Windows

This refactor is expected to span multiple agent sessions. Each session should:

### Before starting work:
1. Read this document (`PROGRESS_KNUTH.md`) to understand overall plan and current status
2. Read `CLAUDE.md` for build instructions and API conventions
3. Check which checkboxes are marked `[x]` to know what's done
4. Read any `## Session Notes` entries at the bottom for context from previous sessions

### While working:
1. Mark checkboxes `[x]` as tasks are completed
2. Run `cargo test --release` after each significant change
3. Run `cargo clippy --release` before considering a phase complete
4. Use `PATH="$HOME/.cargo/bin:$PATH"` prefix for all cargo commands

### Before ending a session:
1. Update checkboxes in this document
2. Add a `### Session N` entry under `## Session Notes` below with:
   - What was accomplished
   - What's next (the immediate next task)
   - Any tricky issues encountered or design decisions made
   - Any test failures or known issues
3. Commit all changes

### Critical invariants to maintain:
- **The game must remain runnable** after each phase (or at least each sub-phase
  of Phase 4). Don't break the build for extended periods.
- **Tests must pass** within each phase before moving to the next.
- **The Python reference** (`extern/knuth-bendix.py`) is ground truth for the
  algebraic layer. If Rust and Python disagree, Python is right.

### File locations (planned):
```
src/hyperbolic/
  knuth_bendix.rs    ← Phase 1 (new)
  cell_id.rs         ← Phase 2 (new)
  cell_registry.rs   ← Phase 3 (new)
  tiling.rs          ← Phase 4 (gutted/replaced)
  poincare.rs        ← Kept (Mobius transforms still needed)
  embedding.rs       ← Kept (rendering geometry)
  mod.rs             ← Updated to export new modules
```

---

## Branch

All work for this refactor happens on the **`knuth`** branch (based off `game`).
Always check out `knuth` before starting a session on this workstream.

---

## Session Notes

_(To be filled in by each agent session that works on this refactor.)_
