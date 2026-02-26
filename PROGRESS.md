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

- [x] Add `slotmap` and `smallvec` to Cargo.toml
- [x] Define `TileAddr = SmallVec<[u8; 12]>` type alias
- [x] Replace `Vec<u8>` with `TileAddr` in `Tile`, `WorldState`, `CellState`, `BeltDrag`, etc.
- [x] Define `EntityId` via `slotmap::SlotMap`
- [x] Rewrite `WorldState`: `tile_grid` + `structures` SlotMap + `positions`/`directions` SecondaryMaps
- [x] Define `StructureKind` enum: `Belt(BeltId)`, `Machine(MachineId)`, `PowerNode`, etc.
- [x] Update all callers in `app.rs`: `try_place_at`, `handle_placement_click`, `handle_placement_drag`
- [x] Update belt overlay rendering to use new world queries
- [x] Extract `UiState` struct from `App` (flash, drag, cursor, panel open flags)

## Phase 3: Belt Simulation

> Items move on belts. The core factory gameplay loop begins.

- [x] Create `sim/belt.rs` with `TransportLine`, `BeltNetwork`, `BeltEnd`
- [x] Implement gap-based item storage and `tick()` method
- [x] Implement `fast_forward(elapsed_ticks)` for chunk catch-up
- [x] Build transport line topology when belts are placed/removed
- [x] Same-direction consecutive segments within a tile merge into one line
- [x] Cross-tile connections via `BeltEnd::Belt` links
- [x] Add belt input/output: items can be placed on belt input end, taken from output end
- [x] Wire belt simulation into simulation tick
- [x] Debug visualization: show item positions on belts
- [x] Debug item spawner: place items on belt via click

## Phase 4: Machine & Inserter Simulation

> Complete the production chain: belts -> inserters -> machines -> inserters -> belts.

- [x] Create `sim/machine.rs` with `MachinePool` (SoA hot/cold split)
- [x] Implement machine state machine: Idle -> Working -> OutputFull / NoInput / NoPower
- [x] Create `sim/inserter.rs` with `InserterPool`
- [x] Implement inserter grab/place logic with two-phase transfer
- [x] Wire machines and inserters into simulation tick
- [x] Auto-create inserters when machine placed adjacent to belt (or: separate placeable item)
- [x] Machine UI: click machine to see recipe, progress, input/output slots
- [x] Recipe selection UI for placed machines
- [x] Source machine, debug-only, for producing any item the user chooses

## Phase 5: Power Network

> Machines require power. Quadrupoles supply it.

- [x] Create `sim/power.rs` with `PowerNetwork`
- [x] Implement connected-component power solving (ratio-based)
- [x] Wire power into simulation tick
- [x] Machines without power enter `NoPower` state
- [x] Power overlay visualization (satisfaction ratio as color)
- [x] Power info in machine UI

## Phase 5b: Split Power Production from Distribution

> Quadrupoles transmit power, Dynamos produce it. Quadrupoles alone don't generate energy.

- [x] Change Quadrupole from Producer to Relay (transmits but produces 0 power)
- [x] Dynamo is the sole power producer (rate 8.0)
- [x] Relay nodes extend the power graph: machines connect to relays, relays connect to relays and dynamos
- [x] Update power overlay to distinguish relays (no pip) from producers (bright pip)
- [x] Update item descriptions to reflect Quadrupole=transmitter, Dynamo=generator

## Phase 6: Instanced Rendering

> Replace per-tile draw calls and egui belt overlay with instanced rendering.

- [x] Define instance buffer types: `TileInstance`, `BeltInstance`, `MachineInstance`, `ItemInstance`
- [x] Create `InstanceBuffer<T>` helper (staging Vec + GPU buffer + upload)
- [x] Create `RenderEngine` struct, extract rendering from `app.rs`
- [x] Write `tile.wgsl` shader with per-instance Mobius from vertex buffer
- [x] Write `belt.wgsl` shader: position rectangle on tile surface via instance data
- [x] Write `machine.wgsl` shader with machine-type-specific visuals
- [x] Write `item.wgsl` shader: billboard or sprite at belt position
- [x] Shared WGSL functions: `apply_mobius()`, `disk_to_bowl()`, `klein_to_poincare()`
- [x] Build instance buffers each frame from visible tiles + world state
- [x] Delete egui belt overlay code
- [x] Delete per-tile uniform buffer and dynamic offset system

## Phase 7: Multi-Cell Machines

> Support machines with footprints larger than 1x1 grid cells.


- [x] Enable click sensitivity on non-occluded cuboid representing a machine.
- [x] Extend `PortDef` with grid offset for ports on non-origin cells
- [x] Update `WorldState` to register multi-cell machines across all occupied cells
- [x] Update placement logic to check entire footprint is free
- [x] Update rotation to transform cell offsets as well as port directions
- [x] Define footprints per machine type (e.g., Embedder 1x2, Transformer 2x2)
- [x] Update belt connection logic to check ports on exterior cells only
- [ ] Update rendering for multi-cell machine meshes
- [ ] Standardize on square footprints (3x2 machines become 3x3), add rotate-in-place for placed buildings



## Phase 9: Chunk Streaming

> Support unbounded world exploration without running out of memory.

- [ ] Create `game/chunk.rs` with `ChunkManager`, `Chunk`, `ChunkAddr`
- [ ] Define chunk boundaries by address prefix depth
- [ ] Implement ring-based loading centered on player
- [ ] Implement LRU eviction with sim state serialization
- [ ] Implement freeze/thaw with fast-forward catch-up
- [ ] Integrate with `TilingState`: on-demand tile generation instead of global BFS
- [ ] Integrate with simulation: only tick Active/Nearby chunks



## Phase 10: Save/Load

> Persist world state across sessions.

- [ ] Create `game/save.rs`
- [ ] Define save format: world state + chunk states + inventory + camera position
- [ ] Serialize with `bincode` or `rmp-serde`
- [ ] Auto-save on exit, manual save via UI
- [ ] Load on startup if save file exists
- [ ] Version the save format for forward compatibility
