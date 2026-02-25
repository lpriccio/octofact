# Octofact — Progress

## Phase 1: Fixed Timestep & Camera Extraction

> Decouple simulation from rendering. Establish the tick/render boundary.

- [x] Extract `Camera` struct from `App` (fields: tile, local, heading, height, mode)
- [x] Move `process_movement()`, `build_view_proj()`, `unproject_to_disk()` to `Camera`
- [x] Add `GameLoop` struct with accumulator, `sim_tick` counter
- [x] Wrap simulation in `about_to_wait()` with fixed timestep loop
- [x] Interpolate camera position between ticks for smooth rendering
- [x] Add `sim/mod.rs` and `sim/tick.rs` with `FixedTimestep` logic
- [x] Debug overlay showing UPS and FPS independently

## Phase 2: World Rewrite & Entity IDs

> Replace nested HashMaps with typed pools and stable entity IDs.

- [ ] Add `slotmap` and `smallvec` to Cargo.toml
- [ ] Define `TileAddr = SmallVec<[u8; 12]>` type alias
- [ ] Replace `Vec<u8>` with `TileAddr` in `Tile`, `WorldState`, `CellState`, `BeltDrag`, etc.
- [ ] Define `EntityId` via `slotmap::SlotMap`
- [ ] Rewrite `WorldState`: `tile_grid` + `structures` SlotMap + `positions`/`directions` SecondaryMaps
- [ ] Define `StructureKind` enum: `Belt(BeltId)`, `Machine(MachineId)`, `PowerNode`, etc.
- [ ] Update all callers in `app.rs`: `try_place_at`, `handle_placement_click`, `handle_placement_drag`
- [ ] Update belt overlay rendering to use new world queries
- [ ] Extract `UiState` struct from `App` (flash, drag, cursor, panel open flags)

## Phase 3: Belt Simulation

> Items move on belts. The core factory gameplay loop begins.

- [ ] Create `sim/belt.rs` with `TransportLine`, `BeltNetwork`, `BeltEnd`
- [ ] Implement gap-based item storage and `tick()` method
- [ ] Implement `fast_forward(elapsed_ticks)` for chunk catch-up
- [ ] Build transport line topology when belts are placed/removed
- [ ] Same-direction consecutive segments within a tile merge into one line
- [ ] Cross-tile connections via `BeltEnd::Belt` links
- [ ] Add belt input/output: items can be placed on belt input end, taken from output end
- [ ] Wire belt simulation into simulation tick
- [ ] Debug visualization: show item positions on belts
- [ ] Debug item spawner: place items on belt via click

## Phase 4: Machine & Inserter Simulation

> Complete the production chain: belts -> inserters -> machines -> inserters -> belts.

- [ ] Create `sim/machine.rs` with `MachinePool` (SoA hot/cold split)
- [ ] Implement machine state machine: Idle -> Working -> OutputFull / NoInput / NoPower
- [ ] Create `sim/inserter.rs` with `InserterPool`
- [ ] Implement inserter grab/place logic with two-phase transfer
- [ ] Wire machines and inserters into simulation tick
- [ ] Auto-create inserters when machine placed adjacent to belt (or: separate placeable item)
- [ ] Machine UI: click machine to see recipe, progress, input/output slots
- [ ] Recipe selection UI for placed machines

## Phase 5: Power Network

> Machines require power. Quadrupoles supply it.

- [ ] Create `sim/power.rs` with `PowerNetwork`
- [ ] Implement connected-component power solving (ratio-based)
- [ ] Wire power into simulation tick
- [ ] Machines without power enter `NoPower` state
- [ ] Power overlay visualization (satisfaction ratio as color)
- [ ] Power info in machine UI

## Phase 6: Instanced Rendering

> Replace per-tile draw calls and egui belt overlay with instanced rendering.

- [ ] Define instance buffer types: `TileInstance`, `BeltInstance`, `MachineInstance`, `ItemInstance`
- [ ] Create `InstanceBuffer<T>` helper (staging Vec + GPU buffer + upload)
- [ ] Create `RenderEngine` struct, extract rendering from `app.rs`
- [ ] Write `tile.wgsl` shader with per-instance Mobius from vertex buffer
- [ ] Write `belt.wgsl` shader: position rectangle on tile surface via instance data
- [ ] Write `machine.wgsl` shader with machine-type-specific visuals
- [ ] Write `item.wgsl` shader: billboard or sprite at belt position
- [ ] Shared WGSL functions: `apply_mobius()`, `disk_to_bowl()`, `klein_to_poincare()`
- [ ] Build instance buffers each frame from visible tiles + world state
- [ ] Delete egui belt overlay code
- [ ] Delete per-tile uniform buffer and dynamic offset system

## Phase 7: Chunk Streaming

> Support unbounded world exploration without running out of memory.

- [ ] Create `game/chunk.rs` with `ChunkManager`, `Chunk`, `ChunkAddr`
- [ ] Define chunk boundaries by address prefix depth
- [ ] Implement ring-based loading centered on player
- [ ] Implement LRU eviction with sim state serialization
- [ ] Implement freeze/thaw with fast-forward catch-up
- [ ] Integrate with `TilingState`: on-demand tile generation instead of global BFS
- [ ] Integrate with simulation: only tick Active/Nearby chunks

## Phase 8: Save/Load

> Persist world state across sessions.

- [ ] Create `game/save.rs`
- [ ] Define save format: world state + chunk states + inventory + camera position
- [ ] Serialize with `bincode` or `rmp-serde`
- [ ] Auto-save on exit, manual save via UI
- [ ] Load on startup if save file exists
- [ ] Version the save format for forward compatibility
