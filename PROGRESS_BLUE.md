# Octofact — Blueprint System

> Select placed structures, copy/cut/paste them (Ctrl-C/V/X), save as persistent
> blueprint files, and manage them through an egui UI dialog.

---

## Phase BP1: Selection & Clipboard

> Click-drag to select a rectangle of structures, then Ctrl-C to copy or
> Ctrl-X to cut them into a clipboard buffer.

### Context

`InputState` (input.rs:207) currently only tracks `shift_held`. Blueprint
selection needs a `ctrl_held` flag (Cmd on macOS) to distinguish Ctrl-C/V/X
from plain C/V/X keybinds. `DestroyBuilding` is bound to plain `KeyX`
(input.rs:201), so Ctrl-X won't conflict.

The clipboard stores a list of `BlueprintEntry` structs — each holding a
relative grid offset, `StructureKind`, `Direction`, and optional item contents.
`capture_region()` scans a tile rectangle via `WorldState::tile_entities()`,
deduplicates multi-cell entities (using `is_origin()` to skip non-origin cells),
and records each entity's kind, direction, and relative position.

### Tasks

- [x] Track Ctrl/Cmd modifier in `InputState` — add `ctrl_held: bool` field, update in `keyboard_input()` (input.rs)
- [x] Create `src/game/blueprint.rs` with `BlueprintEntry` struct:
  - `offset: (i32, i32)` — relative grid position within selection
  - `kind: StructureKind`
  - `direction: Direction`
  - `items: Vec<(ItemId, u16)>` — optional stored contents (for storage buildings)
  - `recipe: Option<usize>` — selected recipe index for machines
- [x] Add `Clipboard` struct holding `Vec<BlueprintEntry>`, `width`, `height`, and source `tiling_q`
- [x] Implement `capture_region(world, machine_pool, tile, top_left, bottom_right) -> Clipboard`:
  - Scan rectangle via `WorldState::tile_entities()`
  - Skip non-origin cells (`is_origin()`) to deduplicate multi-cell entities
  - Record each entity's `kind()`, `direction()`, and relative offset
  - For storage entities, snapshot stored items
  - For machines, snapshot recipe selection
- [x] Add selection mode toggle — `B` key enters box-select mode within current tile
- [x] Implement click-drag box selection: track start cell on mouse-down, highlight rectangle on drag, finalize on mouse-up
- [x] Ctrl-C handler: call `capture_region()` with selected rectangle, store result in `App.clipboard`
- [x] Ctrl-X handler: call `capture_region()`, then destroy selected entities — refactor `destroy_at_cursor` (app.rs:972–1082) into reusable `destroy_entity_at()` method that handles all `StructureKind` variants and sim pool unregistration
- [x] Render selection rectangle highlight (tinted overlay on selected cells)
- [x] Unit tests: `capture_region` correctly captures entities with relative offsets
- [x] Unit tests: multi-cell entities (2x2, 3x3) captured once at origin offset
- [x] Unit tests: Ctrl-X removes entities after capturing

### Design Notes

**Selection scope:** Selection supports 1×n tile strips along one axis.
Cross-tile blueprints use virtual coordinates (local + delta*64) with vertex
exclusion at tile corners. Paste detects boundary crossing automatically.

**`destroy_entity_at()` refactor:** The existing `destroy_at_cursor` method
(app.rs:972–1082) does per-`StructureKind` sim pool cleanup (belt network,
machine pool, inserter pool, splitter pool, storage pool). Extracting this into
`destroy_entity_at(entity_id)` benefits both Ctrl-X and any future bulk-removal
features.

---

## Phase BP2: Paste Mode

> Ctrl-V enters paste mode with a ghost preview of the clipboard contents.
> R to rotate, click to place. Collision and inventory checks prevent invalid
> placement.

### Context

Ghost preview already exists for single-structure placement using
`progress = -1.0` as a sentinel in `MachineInstance` (app.rs ~line 1354).
Paste mode extends this to show multiple ghosts simultaneously.
`Direction::rotate_cell()` (world.rs:96) handles rotating offsets within a
bounding box — reuse this for blueprint rotation.

### Tasks

- [x] Implement `Clipboard::rotate_cw()` — rotate all entries 90° clockwise:
  - Transform each entry's offset via `Direction::rotate_cell()` with updated bounding box
  - Rotate each entry's `Direction` one step clockwise
  - Swap clipboard `width` and `height`
- [x] Implement `can_paste(world, tile, anchor) -> Vec<(usize, bool)>`:
  - For each `BlueprintEntry`, check whether target cells are unoccupied
  - Return per-entry pass/fail for granular feedback
- [x] Implement `required_items(clipboard) -> Vec<(ItemId, u16)>`:
  - Tally item costs for all entries (each structure requires its corresponding item)
  - Compare against player inventory
- [x] Ctrl-V handler: enter paste mode, show multi-ghost preview anchored at cursor
  - Reuse ghost instance system with `progress = -1.0` for valid placements
  - Use `progress = -3.0` sentinel for blocked ghosts (collision / missing items)
- [x] Add red tint for blocked ghosts in shader — branch on `progress == -3.0` in `machine.wgsl` fragment stage
- [x] `R` key in paste mode calls `Clipboard::rotate_cw()`, updates preview
- [x] Click to confirm paste — batch placement:
  1. Place non-belt structures first (machines, splitters, storage)
  2. Place belts second (so they can auto-connect to newly placed structures)
  3. Run belt reconnection pass for side-inject and splitter links
- [x] Deduct items from inventory on successful paste
- [x] Unit tests: `rotate_cw()` correctness — offsets and directions after 1, 2, 3, 4 rotations
- [x] Unit tests: `can_paste()` detects collisions with existing structures
- [x] Unit tests: `required_items()` tallies costs correctly
- [x] Unit tests: batch placement order — non-belts before belts

### BP2 Follow-up: Recipe Copying & Multi-Tile Blueprints

- [x] Add `recipe: Option<usize>` to `BlueprintEntry`, snapshot machine recipe on capture, restore on paste
- [x] Virtual coordinate utilities: `virtual_to_tile_local()`, `tile_local_to_virtual()`, `is_vertex_cell()`, `footprint_touches_vertex()`
- [x] Multi-tile selection: `StripTile` struct, `SelectionState.tiles` vec, drag across tile edges
- [x] `capture_strip()` for multi-tile capture with vertex exclusion
- [x] Multi-tile paste: `walk_tile_strip()` helper, tile cache, per-tile collision checks
- [x] Multi-tile ghost preview with per-tile Mobius map
- [x] Cross-tile paste detection: single-tile clipboards pasted near tile edge route through multi-tile path
- [x] Fix `capture_strip` scan range bug: use `clipped_v - delta * 64` instead of `virtual_to_tile_local(clipped_v)`
- [x] Fix selection overlay: render as hyperbolic quad polygon instead of screen-space AABB
- [x] Unit tests: virtual coordinate roundtrip, vertex exclusion, multi-tile rotation

### Design Notes

**Ghost sentinel values:**
- `progress = -1.0` — translucent ghost (valid placement), already used
- `progress = -2.0` — no power (existing usage)
- `progress = -3.0` — blocked ghost (new), rendered with red tint overlay

**Placement order matters:** Belts auto-connect to adjacent structures during
placement. Placing machines first ensures belt endpoints detect and link to
machine ports correctly. The reconnection pass handles any side-inject or
splitter links that require both neighbors to exist.

**Rotation anchor:** The clipboard anchor point is the top-left corner of the
bounding box. Rotation pivots around the bounding box center so the anchor
stays visually stable under the cursor.

---

## Phase BP3: Persistence

> Save clipboard contents to disk as `.blueprint` files and load them back.
> Files are portable between sessions but locked to a specific tiling `q` value.

### Context

The save system (save.rs) already uses bincode + `ProjectDirs` + atomic `.tmp`
writes. Blueprint persistence mirrors this pattern with its own subdirectory
and file extension.

### Tasks

- [ ] Define `BlueprintFile` struct in `blueprint.rs`:
  - `version: u32` — format version for forward compatibility
  - `name: String` — user-provided blueprint name
  - `timestamp: u64` — Unix epoch seconds at save time
  - `tiling_q: u32` — the `q` parameter of the {4,q} tiling
  - `width: u32`, `height: u32` — bounding box dimensions
  - `entries: Vec<BlueprintEntry>` — the structure data
- [ ] Implement `blueprints_dir() -> PathBuf` — `ProjectDirs::data_dir().join("blueprints")`, create on first access
- [ ] Implement `save_blueprint(file: &BlueprintFile) -> Result<PathBuf>`:
  - Serialize with bincode, write atomically via `.tmp` rename (pattern from save.rs)
  - Filename: `{sanitized_name}.blueprint`
- [ ] Implement `load_blueprint(path: &Path) -> Result<BlueprintFile>`:
  - Deserialize with bincode, validate version field
  - Return error if version is unsupported
- [ ] Implement `list_blueprints() -> Vec<(PathBuf, BlueprintFile)>`:
  - Scan `blueprints_dir()` for `.blueprint` files
  - Load and return metadata for each
- [ ] Implement `delete_blueprint(path: &Path) -> Result<()>`
- [ ] Implement `rename_blueprint(path: &Path, new_name: &str) -> Result<PathBuf>`:
  - Update internal name field and rename file on disk
- [ ] Tiling compatibility guard: on load, compare `BlueprintFile.tiling_q` against current world `q` — reject with descriptive error if mismatched
- [ ] Unit tests: round-trip save → load preserves all fields
- [ ] Unit tests: list_blueprints discovers saved files
- [ ] Unit tests: tiling_q mismatch returns error

### Design Notes

**Why lock to tiling `q`:** Structure dimensions and belt lengths are defined in
grid cells, and grid cell geometry changes with `q`. A blueprint saved in {4,5}
would produce incorrect layouts in {4,7}. Rather than attempt remapping, reject
mismatches cleanly.

**File naming:** Sanitize user-provided names (strip path separators, limit
length) to prevent directory traversal or filesystem issues. Collisions append
a numeric suffix.

---

## Phase BP4: Blueprint Manager UI

> An egui window for browsing, previewing, loading, saving, renaming, and
> deleting blueprint files.

### Context

Existing UI windows in `src/ui/` follow a consistent pattern: a public function
returning `Option<SomeAction>` that the caller dispatches. The blueprint manager
follows this same pattern, returning `Option<BlueprintAction>` for the app to
handle.

### Tasks

- [ ] Create `src/ui/blueprint.rs` following existing window patterns (inventory.rs, tech_tree.rs)
- [ ] Define `BlueprintAction` enum: `LoadToClipboard(PathBuf)`, `SaveClipboard(String)`, `Rename(PathBuf, String)`, `Delete(PathBuf)`
- [ ] Left panel: scrollable list of saved blueprints with metadata columns:
  - Name, dimensions (W×H), entity count, save date
  - Click to select, highlight selected row
- [ ] Right panel: 2D top-down schematic preview of selected blueprint:
  - Render via `egui::Painter` — colored rectangles for structures, arrows for belt direction
  - Scale to fit panel, maintain aspect ratio
- [ ] Action buttons: Load to Clipboard, Save Current Clipboard, Rename, Delete
  - Save prompts for a name via inline text input
  - Delete prompts for confirmation
- [ ] Item cost summary table: list all required items and quantities for the selected blueprint
- [ ] Keybind hints footer: display Ctrl-C / Ctrl-V / Ctrl-X (Cmd on macOS) shortcuts
- [ ] Wire blueprint manager into app — add keybind to open/close the window
- [ ] Register `src/ui/blueprint.rs` in `src/ui/mod.rs`

### Design Notes

**Preview rendering:** The schematic preview is intentionally simple — flat
colored rectangles per structure kind, with directional arrows for belts. This
avoids pulling in the full render pipeline and keeps the UI responsive. Each
`StructureKind` maps to a distinct color matching the build menu palette.

**Action dispatch:** The UI function is pure — it returns actions but doesn't
mutate world state. The caller (`App`) matches on `BlueprintAction` and
performs the actual file I/O or clipboard operations. This keeps the UI
testable and decoupled.

---

## Dependencies

```
Phase BP1 (selection & clipboard) ─── no dependencies, can start immediately
Phase BP2 (paste mode)            ─── depends on BP1 (needs Clipboard struct)
Phase BP3 (persistence)           ─── depends on BP1 (needs BlueprintEntry/Clipboard)
Phase BP4 (manager UI)            ─── depends on BP3 (needs save/load/list functions)
                                       and BP1 (needs Clipboard for "Save Current")
```

```
BP1 ──→ BP2
  │
  └──→ BP3 ──→ BP4
```
