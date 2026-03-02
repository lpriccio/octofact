# Knuth-Bendix Cell Identity Refactor

**Branch**: `knuth`
**Status**: Phase 4 — Phases 1, 2 & 3 complete
**Goal**: Replace floating-point spatial dedup with exact algebraic cell identity using confluent rewrite rules, enabling reliable exploration far from origin.

---

## Problem

The current `TilingState` identifies cells via `spatial_key()`: discretizing Poincare disk positions to a grid. This works near the origin but suffers from floating-point drift when exploring far out and returning — two paths to the same cell produce slightly different coordinates, creating duplicate or missing tiles.

## Solution

Use the Knuth-Bendix completion algorithm's confluent rewrite rules to reduce any path (word) through the tiling to a unique canonical form. This canonical form becomes the **CellId** — an exact, algebraic cell identity immune to floating-point drift.

## Alphabet & Generators

The turtle sits at the center of a cell, looking at one edge.

| Letter | Meaning | Byte encoding |
|--------|---------|---------------|
| `a` | Move forward through faced edge (involution: `aa = e`) | `0` |
| `B` | Turn left one edge | `1` |
| `b` | Turn right one edge | `2` |

The letter `A` (uppercase) is immediately rewritten to `a` (rule 4). The effective alphabet is `{a, B, b}`.

## Rewrite Rules (from `extern/rewrite-pairs.txt`)

These 11 confluent rules for {4,5} were pre-computed via Knuth-Bendix completion from the fundamental relations `a^2 = B^4 = (aB)^5 = e`, `bB = Bb = e`:

| # | LHS | RHS | Notes |
|---|-----|-----|-------|
| 1 | `bB` | (empty) | b and B are inverses |
| 2 | `Bb` | (empty) | b and B are inverses |
| 3 | `aa` | (empty) | a is an involution |
| 4 | `A` | `a` | Normalize uppercase |
| 5 | `bbb` | `B` | 3 right turns = 1 left turn |
| 6 | `BB` | `bb` | 2 left turns = 2 right turns |
| 7 | `ababa` | `BaBaB` | Derived from vertex relation |
| 8 | `aBaBa` | `babab` | Derived from vertex relation |
| 9 | `ababbabab` | `BaBabbaBa` | Derived (longer overlap) |
| 10 | `aBabbaBaB` | `bababbaba` | Derived (longer overlap) |
| 11 | `aBabbaBabb` | `bababbabaB` | Derived (longer overlap) |

**Shortlex ordering**: length first, then lexicographic with `b < B < a`. Every rule's LHS is strictly greater than its RHS in this order, guaranteeing termination.

## Canonicalization

A word encodes both a cell AND an orientation (which edge the turtle faces). The same cell has 4 orientations:

1. Reduce word `X`
2. Also reduce `X·B`, `X·BB`, `X·BBB`
3. The **CellId** is the shortlex minimum of these 4 reduced words
4. The **orientation** is how many B's were appended to reach the minimum (0–3)

This ensures every cell has exactly one canonical representation.

---

## Phase 1: Core Rewrite Engine

**File**: `src/hyperbolic/rewrite.rs`
**Status**: DONE (28 tests passing)
**Depends on**: nothing

Implement the string rewriting engine.

### Deliverables

- [ ] `Word` type: `Vec<u8>` with constants `A_LETTER=0, B_LETTER=1, B_INV=2`
- [ ] `RewriteRule` struct: `lhs: Vec<u8>`, `rhs: Vec<u8>`
- [ ] `fn load_rules_45() -> Vec<RewriteRule>` — hardcode the 11 rules for {4,5}
- [ ] `fn reduce(word: &mut Word, rules: &[RewriteRule])` — apply rules until fixed point
  - Scan left-to-right for first matching LHS substring
  - Replace with RHS
  - Back up scan position by `max(0, rhs.len() - 1)` to catch cascading rewrites
  - Repeat until no rule matches in a full scan
- [ ] `fn shortlex_cmp(a: &Word, b: &Word) -> Ordering` — length first, then lex with b(2) < B(1) < a(0)
- [ ] `fn word_to_string(w: &Word) -> String` and `fn string_to_word(s: &str) -> Word` for display/debugging
- [ ] Parse functions to read `extern/rewrite-pairs.txt` (for validation; runtime uses hardcoded rules)

### Tests

- [ ] Each of the 11 rules individually: input LHS reduces to RHS
- [ ] `aa` → empty
- [ ] `bB` → empty, `Bb` → empty
- [ ] `BBBB` → empty (4 left turns = identity via rules 6, then 1/2)
- [ ] `aBaBaBaBaB` → empty (vertex relation `(aB)^5 = e`)
- [ ] Cascading: `abBa` → empty (inner `bB` cancels, then `aa` cancels)
- [ ] Idempotence: reducing an already-reduced word returns it unchanged
- [ ] Known words from the tiling: verify reduction of specific paths

### Implementation Notes

The rewrite engine is a standard leftmost-innermost strategy. Words for {4,5} are typically short (< 50 chars for cells within rendering distance), so O(n * rules * word_len) per reduction is fine.

---

## Phase 2: CellId & Canonicalization

**File**: `src/hyperbolic/cell_id.rs`
**Status**: DONE (14 tests passing)
**Depends on**: Phase 1

### Deliverables

- [ ] `CellId` struct: wraps a canonical `Word` (the shortlex minimum of 4 orientations)
  - Implements `Eq`, `Hash`, `Ord`, `Clone`, `Debug`
  - Display as the string form of the canonical word (empty = "e" for origin)
- [ ] `OrientedCell` struct: `CellId` + `orientation: u8` (0–3)
  - Represents a specific turtle state (cell + facing direction)
- [ ] `fn canonicalize(word: &Word, rules: &[RewriteRule]) -> (CellId, u8)`
  - Reduce `word`, `word·B`, `word·BB`, `word·BBB`
  - Return (shortlex minimum, index of minimum)
- [ ] `fn neighbor(cell: &CellId, orientation: u8, edge: u8, rules: &[RewriteRule]) -> OrientedCell`
  - Compute `word · B^((edge - orientation) mod 4) · a`
  - Reduce and canonicalize
  - The returned orientation tells which edge of the neighbor connects back
- [ ] `fn all_neighbors(cell: &CellId, rules: &[RewriteRule]) -> [(CellId, u8); 4]`
  - Compute all 4 neighbors (one per edge) using canonical orientation (0)

### Tests

- [ ] Origin cell: empty word canonicalizes to `(CellId(""), 0)`
- [ ] All 4 rotations of origin → same CellId
- [ ] `a` (cross edge 0) and `Ba` (turn left, cross) give different CellIds (different cells)
- [ ] Round-trip: neighbor of neighbor across the same edge returns to original CellId
- [ ] Vertex consistency: 5 successive `neighbor(edge=k)` calls around a vertex return to original cell
- [ ] BFS expansion to depth N produces correct number of unique CellIds
  - Depth 1: 4 neighbors + origin = 5
  - Depth 2: 17 (verified)
- [ ] CellId equality: two different paths to the same cell produce the same CellId
  - e.g., `aBa` and `bab` (if they represent the same cell — verify)

---

## Phase 3: Algebraic Neighbor Graph (CellGraph)

**File**: `src/hyperbolic/cell_graph.rs`
**Status**: DONE (22 tests passing)
**Depends on**: Phase 2

Build a graph of discovered cells with precomputed neighbor relationships.

### Deliverables

- [x] `CellData` struct:
  - `id: CellId`
  - `neighbors: [CellId; 4]` — direct neighbors (one per edge)
  - `neighbor_orientations: [u8; 4]` — which edge of each neighbor connects back
  - `mobius: Mobius` — transform mapping origin polygon to this cell (for rendering)
  - `parity: bool` — even/odd (word has even/odd number of `a` letters)
- [x] `CellGraph` struct:
  - `cells: HashMap<CellId, CellData>`
  - `rules: Vec<RewriteRule>` (cached)
  - `origin: CellId` (always the empty word)
- [x] `fn expand_bfs(&mut self, center: &CellId, radius: usize)`
  - BFS from center, expanding `radius` hops out
  - Compute and store neighbors + Mobius for each new cell
  - `radius=1`: immediate neighbors only (for simulation)
  - `radius=3`: extended neighborhood (for rendering)
- [x] `fn ensure_neighborhood(&mut self, cell: &CellId, radius: usize)`
  - Ensure all cells within `radius` hops of `cell` are loaded
  - Idempotent: skip already-loaded cells
- [x] `fn cells_within(&self, center: &CellId, radius: usize) -> Vec<&CellId>`
  - Return all loaded cells within `radius` BFS hops of `center`

### Mobius Computation from Words

For rendering, each cell needs a Mobius transform. Compute from the canonical word:

```
fn word_to_mobius(word: &Word, neighbor_xforms: &[Vec<Mobius>; 2]) -> Mobius {
    let mut facing: u8 = 0;
    let mut transform = Mobius::identity();
    let mut parity = false;
    for &letter in word {
        match letter {
            A_LETTER => {  // 'a'
                transform = transform.compose(&neighbor_xforms[parity as usize][facing as usize]);
                parity = !parity;
                facing = (facing + 2) % 4;  // flip: face back toward where we came
            }
            B_LETTER => {  // 'B' = turn left (always +1, parity-independent)
                facing = (facing + 1) % 4;
            }
            B_INV => {  // 'b' = turn right (always -1, parity-independent)
                facing = (facing + 3) % 4;
            }
            _ => unreachable!()
        }
    }
    transform
}
```

Verify early: `(aB)^5` and `aa` must compose to Mobius identity. BFS cells must match positions from the existing spatial system.

### Tests

- [x] BFS from origin, depth 3: all cells have valid neighbors (no dangling refs)
- [x] Every cell's neighbor's neighbor (across the same edge) is the original cell
- [x] Mobius from word matches Mobius from incremental BFS (within 1e-10)
- [x] Cell count at depth N matches existing TilingState expansion
- [x] No duplicate CellIds in BFS

---

## Phase 4: Comprehensive Algebraic Tests

**Status**: NOT STARTED
**Depends on**: Phase 3

**STOP HERE before touching the rendering/game layer.** This phase ensures the algebraic system is rock-solid.

### Test Suite

- [ ] **Long walk test**: Walk 100 steps in one direction, then 100 steps back. Final CellId = origin.
- [ ] **Random walk test**: Random sequence of 1000 turtle moves, verify:
  - Every intermediate CellId is valid (reduces to itself)
  - All neighbors exist and are consistent
  - No panics or infinite loops in reduction
- [ ] **Cross-validation**: Expand BFS to depth 5 with BOTH old spatial system and new algebraic system. Verify 1:1 correspondence of cells. Every spatial position maps to exactly one CellId and vice versa.
- [ ] **Orientation consistency**: For every cell, verify that all 4 orientations produce the same CellId.
- [ ] **Neighbor symmetry**: If A is neighbor of B across edge j, then B is neighbor of A across some edge k. Verify for all cells in BFS depth 4.
- [ ] **Growth rate**: {4,5} exponential growth. Verified so far: depth 0→3 = 1, 5, 17, 45. Continue to depth 5+.

### Benchmark

- [ ] Time `reduce()` for words of length 10, 50, 100, 500
- [ ] Time `canonicalize()` for same lengths
- [ ] Time BFS expansion to depth 5, 8, 10
- [ ] Ensure reduction of typical words (length < 50) takes < 1ms

---

## Phase 5: Integration — TilingState Refactor

**File**: `src/hyperbolic/tiling.rs` (major rewrite)
**Status**: NOT STARTED
**Depends on**: Phase 4 (all algebraic tests passing)

Replace spatial dedup with CellId-based identity. Keep Mobius transforms for rendering.

### Changes

- [ ] `Tile` struct:
  - Replace `address: TileAddr` with `id: CellId`
  - Keep `transform: Mobius` (still needed for rendering)
  - Keep `parity: bool`
  - Add `neighbors: [Option<CellId>; 4]` (direct neighbors, populated on expansion)
  - Remove `depth: usize` (can derive from CellId word length if needed)
- [ ] `TilingState`:
  - Replace `seen: HashSet<(i64, i64)>` with `seen: HashSet<CellId>`
  - Replace `spatial_to_tile: HashMap<(i64, i64), usize>` with `id_to_tile: HashMap<CellId, usize>`
  - Add `rules: Vec<RewriteRule>` (cached)
  - Keep `frontier: VecDeque<usize>` (still BFS-based for incremental expansion)
- [ ] `expand_near()`: Use CellId for dedup instead of spatial_key
  - When expanding a parent, compute child CellId algebraically
  - Check `seen` by CellId instead of spatial position
  - Still compute child Mobius incrementally (compose parent transform with neighbor xform)
- [ ] `recenter_on()`:
  - CellIds are absolute — they don't change on recenter!
  - Only Mobius transforms need updating (as now)
  - Remove address recomputation logic
  - `id_to_tile` map stays valid (just update indices after compaction)
- [ ] Remove `reduce_address()` (replaced by algebraic reduction)
- [ ] Remove `spatial_key()` (no longer needed for dedup)
- [ ] `find_tile_near()`:
  - Can still use spatial lookup as a fast path for rendering
  - But primary lookup should be by CellId: `fn find_tile(&self, id: &CellId) -> Option<usize>`
- [ ] Keep a spatial index as a secondary/convenience index for click detection

### Tests

- [ ] All existing TilingState tests still pass (adapted to new types)
- [ ] Walk 100 steps and return: origin tile has CellId = empty word
- [ ] No duplicate CellIds after any sequence of expand + recenter operations
- [ ] Recenter preserves CellId→Mobius consistency

---

## Phase 6: Integration — WorldState & Camera

**Files**: `src/game/world.rs`, `src/render/camera.rs`
**Status**: NOT STARTED
**Depends on**: Phase 5

### WorldState Changes

- [ ] `GridPos.tile`: change from `TileAddr` to `CellId`
- [ ] `tile_grid`: change key from `TileAddr` to `CellId`
- [ ] All methods using `&[u8]` address slices → use `&CellId`
- [ ] `place()`, `remove()`, `tile_entities()` etc. — update signatures

### Camera Changes

- [ ] `Camera.tile`: change from `usize` (tile index) to `CellId`
- [ ] Camera movement: compute next cell algebraically
  - When crossing a tile boundary, compute the new CellId using `neighbor()`
  - No need for `find_tile_near()` — the algebraic system tells us exactly which cell
- [ ] Keep `Camera.local: Mobius` for sub-tile positioning (where within the current cell)
- [ ] `CameraSnapshot`: update to use CellId

### Tests

- [ ] Camera can traverse 200+ tiles and return to origin with correct CellId
- [ ] WorldState placement and removal work with CellId keys
- [ ] Camera recentering maintains correct CellId

---

## Phase 7: Integration — App & Rendering

**Files**: `src/app.rs`, `src/render/engine.rs`, various UI files
**Status**: NOT STARTED
**Depends on**: Phase 6

### Changes

- [ ] `App::update_movement()`: use algebraic neighbor computation for tile transitions
- [ ] `App::render()`: look up tiles by CellId in the render loop
- [ ] Debug HUD: display CellId string instead of old address format
- [ ] Click/hover detection: use algebraic neighbor lookup
- [ ] Belt/machine placement: use CellId in all entity references
- [ ] `format_address()` → `format_cell_id()` (display canonical word)

### Tests

- [ ] Full integration test: launch, walk around, place structures, walk far, return
- [ ] Visual verification: no tile popping, tearing, or gaps
- [ ] Performance: frame time stays under 16ms during exploration

---

## Handoff Notes for Future Claude Instances

### Context Window Management

This refactor spans 7 phases and will likely require multiple sessions. Each session should:

1. **Read this file first** to understand current status
2. **Check the latest test results**: `PATH="$HOME/.cargo/bin:$PATH" cargo test --release 2>&1 | tail -20`
3. **Read the files relevant to the current phase** (listed in each phase section)
4. **Update this file** when completing milestones (change `NOT STARTED` → `IN PROGRESS` → `DONE`)
5. **Add notes below** about any surprises, bugs, or design decisions

### Key Files

| File | Role |
|------|------|
| `src/hyperbolic/rewrite.rs` | NEW — Rewrite engine (Phase 1) |
| `src/hyperbolic/cell_id.rs` | NEW — CellId, canonicalization, neighbors (Phase 2–3) |
| `src/hyperbolic/tiling.rs` | MODIFY — Replace spatial dedup with CellId (Phase 5) |
| `src/hyperbolic/poincare.rs` | READ ONLY — Mobius, Complex types, neighbor_xforms |
| `src/game/world.rs` | MODIFY — TileAddr → CellId (Phase 6) |
| `src/render/camera.rs` | MODIFY — Tile index → CellId (Phase 6) |
| `src/app.rs` | MODIFY — Wire everything together (Phase 7) |
| `extern/rewrite-pairs.txt` | Reference — The 11 confluent rewrite rules |

### Turn Direction: Parity-Independent (CONFIRMED)

`B` (turn left) **always** means `facing = (facing + 1) % 4`, regardless of parity. `b` (turn right) always means `facing = (facing - 1) % 4`. The parity flip from crossing an edge does NOT reverse the turn direction. Verified by user.

### Build Commands

```sh
PATH="$HOME/.cargo/bin:$PATH" cargo test --release                    # all tests
PATH="$HOME/.cargo/bin:$PATH" cargo test --release rewrite            # Phase 1 tests
PATH="$HOME/.cargo/bin:$PATH" cargo test --release cell_id            # Phase 2-3 tests
PATH="$HOME/.cargo/bin:$PATH" cargo test --release tiling             # Phase 5 tests
PATH="$HOME/.cargo/bin:$PATH" cargo clippy --release                  # lint
PATH="$HOME/.cargo/bin:$PATH" cargo run --release                     # visual check
```

### Design Decision Log

_Record decisions here as they are made:_

- **Byte encoding**: `a=0, B=1, b=2`. Shortlex comparison: length first, then lex with custom order `b(2) < B(1) < a(0)` — implement via mapped comparison, not raw byte order.
- **{4,5} only**: Rules are hardcoded for {4,5}. Generalizing to other {4,q} requires re-running Knuth-Bendix completion (out of scope).
- **Incremental Mobius**: Cell Mobius transforms are computed incrementally during BFS (compose parent + neighbor xform), NOT from the word. Word→Mobius is used only for verification.
- **CellIds are absolute**: They don't change on recenter. Only Mobius transforms are recentered.

### Session Log

_Each session should add an entry here:_

```
[2026-03-02] [Phase 1-2] [DONE]
Implemented rewrite.rs (10 rules, reduce(), shortlex_cmp, string conversion) and
cell_id.rs (CellId, OrientedCell, canonicalize, neighbor, all_neighbors).
42 tests passing. BFS depth counts: 1, 5, 17, 45.
Bug found & fixed: rule 4 (A→a) caused infinite loop since both encode to byte 0.
Omitted from rule set since parse already normalizes A→a.
Turn direction confirmed parity-independent: B always means facing+1.
Next: Phase 3 (CellGraph with Mobius) and Phase 4 (comprehensive tests).

[2026-03-02] [Phase 3] [DONE]
Implemented cell_graph.rs: CellData, CellGraph, word_to_mobius, expand_bfs,
ensure_neighborhood, cells_within. 22 tests passing.
Key design: Mobius computed from canonical word via word_to_mobius (not incremental),
verified against incremental BFS composition within 1e-10.
Back-edges (neighbor_orientations) computed by brute-force check of all 4 neighbor edges.
B (turn) only changes facing, not Mobius — all 4 orientations of a cell share the same transform.
Added #![allow(dead_code)] to rewrite/cell_id/cell_graph modules (used only in tests until Phase 5).
Next: Phase 4 (comprehensive algebraic tests).
```
