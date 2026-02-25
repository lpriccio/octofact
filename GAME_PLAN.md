# Octofact Game Architecture Plan

> Factory game on a {4,n} hyperbolic plane. Rust + wgpu 28 + Metal.
> This document is the blueprint for turning the current rendering prototype
> into a playable factory game.

---

## Table of Contents

1. [Current State](#1-current-state)
2. [Target Architecture](#2-target-architecture)
3. [Type Hierarchy](#3-type-hierarchy)
4. [Core Loop Pseudocode](#4-core-loop-pseudocode)
5. [System-by-System Design](#5-system-by-system-design)
6. [Rendering Overhaul](#6-rendering-overhaul)
7. [Dependencies & Conflicts](#7-dependencies--conflicts)
8. [Implementation Phases](#8-implementation-phases)
9. [Open Questions](#9-open-questions)

---

## 1. Current State

### What exists and works well

| Layer | Status | Notes |
|-------|--------|-------|
| Hyperbolic math | Solid | Mobius, canonical addressing, BFS tiling, Klein grid |
| Camera & movement | Solid | First-person + top-down, rebase on tile change |
| Tile rendering | Works but unscalable | 1 draw call per tile, MAX_TILES=1024 |
| Belt placement | Works | Click + drag-to-place on grid, direction, rotation |
| Inventory / recipes | Data defined | 26 items, 23 recipes, 5 machine types |
| UI | Functional | Settings, inventory, placement panel, tooltips |
| Config persistence | Works | TOML at ~/.config/octofact/ |

### What's missing

- **No simulation** — belts don't move items, machines don't craft
- **No tick system** — frame-driven only, no fixed timestep
- **No entity IDs** — structures stored in nested HashMaps
- **No batched rendering** — belts are egui overlays, tiles are individual draws
- **No chunk streaming** — all tiles in memory forever
- **No power network** — quadrupoles exist but do nothing
- **No save/load**

### Scalability bottlenecks

| Bottleneck | Where | Impact |
|------------|-------|--------|
| 1 draw call per visible tile | `app.rs:1047-1051` | GPU command overhead, caps at ~1024 tiles |
| Belt overlay via egui shapes | `app.rs:857-987` | Re-tessellated every frame, O(visible_belts) |
| `Vec<u8>` address cloning | `world.rs` every operation | O(depth) allocation per place/lookup |
| Nested `HashMap<Vec<u8>, HashMap<(i32,i32), Structure>>` | `world.rs` | No entity IDs, no iteration by type, no spatial queries |
| Per-frame `visible` Vec allocation | `app.rs:728` | Heap alloc every frame |
| `recenter_on` recomputes all tiles | `tiling.rs:156` | O(num_tiles) on every rebase |
| No simulation decoupling | everywhere | Can't independently tune UPS vs FPS |

---

## 2. Target Architecture

### Design Principles

1. **Typed pools over ECS.** We have ~6 entity types (belts, machines, inserters,
   items-on-ground, power nodes, pipes). Each has known, fixed components. A full
   ECS adds dynamic dispatch overhead without benefit. Use dense `Vec<T>` per type
   with `SlotMap` for stable IDs.

2. **Gap-based belt simulation.** Factorio's transport line optimization: store
   distances between items, not positions. O(1) per unblocked line per tick.

3. **Phase-based parallel ticks.** Parallelize *within* each system (all machines
   in parallel), not *across* systems. Sequential phase boundaries prevent races.
   Rayon `par_iter_mut` enforces this at compile time.

4. **Instanced rendering.** One draw call per entity type per frame. Per-instance
   data (Mobius transform, direction, animation phase) in vertex buffers.

5. **Fixed timestep simulation.** 60 UPS independent of FPS. Accumulator pattern
   with interpolation for camera only.

6. **Cell-relative coordinates always.** Every entity is `(TileAddr, grid_x, grid_y)`.
   Never global Poincare/Klein coords in game state. Mobius reconstructs position
   on demand for rendering.

7. **Chunk streaming.** Address-prefix chunks. Ring loading around player. LRU
   eviction. Freeze distant chunks; fast-forward on approach.

### Module Layout (target)

```
src/
  main.rs                       # CLI args, EventLoop
  app.rs                        # Slim: owns GameState + RenderEngine, dispatches events
  sim/
    mod.rs
    tick.rs                     # FixedTimestep, GameLoop with accumulator
    belt.rs                     # TransportLine, BeltNetwork (gap-based)
    machine.rs                  # MachinePool (SoA hot/cold split)
    inserter.rs                 # InserterPool, transfer logic
    power.rs                    # PowerNetwork (graph solve)
    logistics.rs                # High-level item flow coordination
  game/
    mod.rs
    config.rs                   # (unchanged)
    input.rs                    # (unchanged)
    items.rs                    # (unchanged)
    inventory.rs                # (unchanged)
    recipes.rs                  # (unchanged)
    world.rs                    # Rewritten: EntityId, TileAddr, typed pools
    chunk.rs                    # ChunkManager, streaming, freeze/thaw
    save.rs                     # Serialization
  hyperbolic/
    mod.rs
    poincare.rs                 # (unchanged)
    tiling.rs                   # (minor: SmallVec addresses, persistent visible set)
    embedding.rs                # (unchanged)
  render/
    mod.rs
    mesh.rs                     # (add instance buffer layouts)
    pipeline.rs                 # (rewrite: instanced pipelines per entity type)
    camera.rs                   # (extract from app.rs)
    instances.rs                # Instance buffer management, upload
    shader.wgsl                 # (split: tile.wgsl, belt.wgsl, machine.wgsl)
  ui/
    (unchanged, add crafting UI later)
```

---

## 3. Type Hierarchy

### 3.1 Entity Identity

```
TileAddr (SmallVec<[u8; 12]>)
  - Replaces Vec<u8> everywhere
  - 12 bytes inline = depth 12 without heap alloc
  - Covers ~4^12 = 16M tiles before spilling

EntityId (u32)
  - Opaque handle into a SlotMap
  - Stable across removals (generational index)
  - Zero-cost to copy, hash, compare

GridPos { tile: TileAddr, gx: i16, gy: i16 }
  - Canonical position of any placed entity
  - i16 range [-32768..32767] vs current 64x64 grid = plenty of room
```

### 3.2 World State (rewrite of game/world.rs)

```
WorldState
  +-- tile_grid: HashMap<TileAddr, TileSlots>
  |     TileSlots: array or HashMap of GridPos -> EntityId
  |     "What's at this grid square?"
  |
  +-- structures: SlotMap<EntityId, StructureKind>
  |     StructureKind: enum { Belt(BeltId), Machine(MachineId), Inserter(InserterId), ... }
  |     "What type is this entity?"
  |
  +-- positions: SecondaryMap<EntityId, GridPos>
  |     "Where is this entity?"
  |
  +-- directions: SecondaryMap<EntityId, Direction>
        "Which way does it face?"
```

### 3.3 Simulation Types (new: sim/)

```
BeltNetwork
  +-- segments: SlotMap<BeltId, BeltSegment>
  |     BeltSegment { line: TransportLineId, position_in_line: u16 }
  |
  +-- lines: SlotMap<TransportLineId, TransportLine>
  |     TransportLine {
  |       gaps: Vec<u16>,       // fixed-point distances between items
  |       items: Vec<ItemId>,   // item types (len = gaps.len() - 1)
  |       speed: u16,           // tiles/tick in fixed-point
  |       length: u16,          // total length in grid units
  |       input_end: BeltEnd,   // what feeds into this line
  |       output_end: BeltEnd,  // where items exit
  |       last_positive_gap: usize,  // cache: only scan from here
  |     }
  |
  +-- topology: BeltTopology
        Maps belt segments to their transport lines.
        Rebuilt when belts are placed/removed.
        Merges consecutive same-direction segments into single lines.

BeltEnd = enum {
  Open,                         // items fall off / waiting for input
  Machine(EntityId),            // connected to machine input/output
  Belt(TransportLineId),        // feeds into another line
  Splitter(SplitterId),         // future: load balancing
}

MachinePool
  +-- hot: MachineHotData        // touched every tick
  |     progress: Vec<f32>       // [0..1] crafting completion
  |     recipe_ticks: Vec<u16>   // ticks remaining
  |     power_draw: Vec<f32>     // current power satisfaction [0..1]
  |     state: Vec<MachineState> // Idle | Working | OutputFull | NoPower
  |
  +-- cold: MachineColdData      // touched on interaction
  |     recipe: Vec<Option<RecipeId>>
  |     input_slots: Vec<[ItemStack; 4]>
  |     output_slots: Vec<[ItemStack; 4]>
  |     entity_id: Vec<EntityId>
  |
  +-- count: usize

MachineState = enum { Idle, Working, OutputFull, NoPower, NoInput }

ItemStack { item: ItemId, count: u16 }

InserterId (u32)  // index into inserter pool

InserterPool
  +-- source: Vec<EntityId>      // grab from
  +-- dest: Vec<EntityId>        // place into
  +-- arm_progress: Vec<f32>     // swing animation [0..1]
  +-- state: Vec<InserterState>  // Idle | Grabbing | Placing | Blocked
  +-- held_item: Vec<Option<ItemId>>
  +-- count: usize

PowerNetwork
  +-- nodes: Vec<PowerNode>
  |     PowerNode { entity: EntityId, kind: Producer | Consumer, rate: f32 }
  |
  +-- connections: Vec<(usize, usize)>  // edges in power graph
  +-- satisfaction: Vec<f32>            // per-node power satisfaction ratio
  |
  +-- fn solve(&mut self)
        // Simple ratio: total_production / total_consumption per connected component
        // Every consumer in a component gets the same satisfaction ratio
```

### 3.4 Chunk System (new: game/chunk.rs)

```
ChunkAddr = SmallVec<[u8; 4]>
  // First CHUNK_DEPTH (e.g., 3) bytes of a TileAddr
  // Defines a "neighborhood" of tiles sharing a common prefix

ChunkManager
  +-- loaded: HashMap<ChunkAddr, Chunk>
  +-- player_chunk: ChunkAddr
  +-- load_budget: usize         // max chunks to load per frame
  +-- max_loaded: usize          // LRU eviction threshold
  |
  +-- fn update(&mut self, player_addr: &TileAddr)
  +-- fn get_chunk(&self, addr: &ChunkAddr) -> Option<&Chunk>
  +-- fn is_loaded(&self, addr: &ChunkAddr) -> bool

Chunk
  +-- addr: ChunkAddr
  +-- tiles: Vec<TileAddr>       // all tiles with this prefix
  +-- sim_state: ChunkSimState   // frozen sim data for catch-up
  +-- last_sim_tick: u64         // when this chunk was last ticked
  +-- mesh_dirty: bool           // need to rebuild instance buffers?
  +-- last_accessed: Instant     // for LRU

ChunkSimState
  +-- belt_lines: Vec<TransportLineId>
  +-- machines: Vec<MachineId>
  +-- inserters: Vec<InserterId>

SimScope = enum {
  Active,     // within ~2 chunks of player: full simulation
  Nearby,     // within ~4 chunks: reduced tick rate (every 4th tick)
  Frozen,     // beyond: no simulation, catch-up on approach
}
```

### 3.5 Rendering Types (rewrite of render/)

```
TileInstance {
  mobius_a: [f32; 2],    // Mobius transform a (Complex)
  mobius_b: [f32; 2],    // Mobius transform b (Complex)
  depth: f32,            // tile depth for palette
  elevation: f32,        // terrain height
  _pad: [f32; 2],
}
// 32 bytes per tile instance

BeltInstance {
  mobius_a: [f32; 2],
  mobius_b: [f32; 2],
  grid_offset: [f32; 2], // position within tile (Klein coords, normalized)
  direction: u32,         // 0-3 NESW
  anim_phase: f32,        // item animation offset
}
// 32 bytes per belt instance

MachineInstance {
  mobius_a: [f32; 2],
  mobius_b: [f32; 2],
  grid_offset: [f32; 2],
  machine_type: u32,
  progress: f32,          // crafting progress for animation
}
// 32 bytes per machine instance

ItemInstance {
  mobius_a: [f32; 2],
  mobius_b: [f32; 2],
  grid_offset: [f32; 2],
  item_type: u32,
  _pad: f32,
}
// 32 bytes per item-on-belt instance

RenderEngine
  +-- tile_pipeline: wgpu::RenderPipeline
  +-- belt_pipeline: wgpu::RenderPipeline
  +-- machine_pipeline: wgpu::RenderPipeline
  +-- item_pipeline: wgpu::RenderPipeline
  |
  +-- tile_mesh: MeshBuffers        // shared mesh for all tiles
  +-- belt_mesh: MeshBuffers        // shared mesh for belt segment
  +-- machine_meshes: HashMap<MachineType, MeshBuffers>
  +-- item_mesh: MeshBuffers        // simple quad/sprite
  |
  +-- tile_instances: InstanceBuffer<TileInstance>
  +-- belt_instances: InstanceBuffer<BeltInstance>
  +-- machine_instances: InstanceBuffer<MachineInstance>
  +-- item_instances: InstanceBuffer<ItemInstance>
  |
  +-- global_uniforms: wgpu::Buffer  // view_proj, time, grid_params (shared)
  +-- depth_view: wgpu::TextureView

InstanceBuffer<T>
  +-- buffer: wgpu::Buffer
  +-- staging: Vec<T>              // CPU-side, rebuilt each frame
  +-- capacity: usize
  +-- count: usize                 // how many are valid this frame
  |
  +-- fn clear(&mut self)
  +-- fn push(&mut self, instance: T)
  +-- fn upload(&self, queue: &wgpu::Queue)
```

### 3.6 Camera (extracted from app.rs)

```
Camera
  +-- tile: usize                  // current tile index in TilingState
  +-- local: Mobius                // position within tile
  +-- heading: f64                 // compass direction (radians)
  +-- height: f32                  // eye height
  +-- mode: CameraMode             // FirstPerson | TopDown
  |
  +-- fn view_proj(&self, aspect: f32) -> Mat4
  +-- fn process_movement(&mut self, input: &InputState, tiling: &mut TilingState, dt: f64)
  +-- fn unproject_to_disk(&self, screen_x: f32, screen_y: f32, aspect: f32) -> Option<Complex>
  +-- fn current_tile_addr(&self, tiling: &TilingState) -> &TileAddr
  +-- fn mobius_for_tile(&self, tile_transform: &Mobius) -> Mobius
```

---

## 4. Core Loop Pseudocode

### 4.1 Main Loop (app.rs ApplicationHandler)

```
const SIM_DT: f64 = 1.0 / 60.0;   // 60 UPS
const MAX_FRAME_TIME: f64 = 0.25;   // prevent spiral of death

struct GameLoop {
    accumulator: f64,
    sim_tick: u64,                   // monotonic tick counter
    prev_camera: CameraSnapshot,     // for interpolation
    curr_camera: CameraSnapshot,
}

fn about_to_wait():
    frame_time = min(now - last_frame, MAX_FRAME_TIME)
    last_frame = now

    // --- INPUT PHASE ---
    // (already handled in window_event)

    // --- SIMULATION PHASE ---
    accumulator += frame_time
    while accumulator >= SIM_DT:
        prev_camera = snapshot(camera)
        simulation_tick(SIM_DT)
        curr_camera = snapshot(camera)
        accumulator -= SIM_DT
        sim_tick += 1

    // --- RENDER PHASE ---
    alpha = accumulator / SIM_DT
    render_camera = interpolate(prev_camera, curr_camera, alpha)
    render_frame(render_camera)

    request_redraw()
```

### 4.2 Simulation Tick

```
fn simulation_tick(dt: f64):
    // Phase 1: Camera & World Updates
    camera.process_movement(input_state, tiling, dt)
    chunk_manager.update(camera.current_tile_addr())

    // Phase 2: Power Network
    //   Single-threaded. Memory-bound, parallelism doesn't help.
    //   Compute satisfaction ratio per connected component.
    power_network.solve()

    // Phase 3: Machine Processing
    //   Parallel across all active machines.
    //   Each machine is independent: check inputs, advance progress, produce output.
    for each active machine (par_iter_mut):
        if machine.state == Idle:
            if has_inputs(machine.recipe, machine.input_slots):
                consume_inputs(machine)
                machine.state = Working
                machine.recipe_ticks = recipe.duration
        if machine.state == Working:
            if machine.power_draw > 0:
                machine.recipe_ticks -= 1
                machine.progress = 1.0 - (ticks_left / total_ticks)
                if machine.recipe_ticks == 0:
                    if output_slots_have_room(machine):
                        produce_output(machine)
                        machine.state = Idle
                    else:
                        machine.state = OutputFull
        if machine.state == OutputFull:
            // Inserter will drain output; machine wakes when space freed

    // Phase 4: Inserter Transfers
    //   Two sub-phases:
    //   a) Parallel: each inserter decides what to grab/place (read-only on sources/dests)
    //   b) Sequential: apply transfers (mutates inventories)
    let transfers = inserters.par_iter()
        .filter_map(|ins| ins.plan_transfer(&belt_network, &machine_pool))
        .collect::<Vec<_>>();
    for transfer in transfers:
        apply_transfer(transfer, &mut belt_network, &mut machine_pool)

    // Phase 5: Belt Transport
    //   Parallel across independent transport lines.
    //   Each line: advance items by speed, handle output end.
    belt_network.lines.par_iter_mut().for_each(|line|:
        line.tick()
        // If output_end is a Machine, try to push item
        // If output_end is another Belt, try to transfer
    )
    // Sequential: resolve cross-line transfers
    belt_network.resolve_transfers()

    // Phase 6: Bookkeeping
    input_state.end_frame()
```

### 4.3 Transport Line Tick (Factorio-style)

```
fn TransportLine::tick():
    // Try to move items forward by `speed` units

    // Fast path: if the gap at the output end is >= speed,
    // we can move everything forward without any item reaching the end.
    if gaps[0] >= speed:
        gaps[0] -= speed           // shrink gap at output end
        gaps[len] += speed         // grow gap at input end
        return                     // O(1)!

    // Slow path: items reaching the output end
    // Walk from output end, trying to push items out
    while gaps[0] < speed AND items not empty:
        if can_push_to_output_end():
            item = items.remove(0)
            push_item(output_end, item)
            // Merge gaps[0] and gaps[1]
            gaps[0] += gaps[1]
            gaps.remove(1)
        else:
            // Output blocked: items compress against end
            // Redistribute remaining movement as compression
            gaps[0] = 0
            break

    // Now advance remaining gap
    if gaps[0] >= speed:
        gaps[0] -= speed
        gaps[len] += speed
```

### 4.4 Render Frame

```
fn render_frame(camera: CameraSnapshot):
    view_proj = camera.view_proj(aspect)
    upload_global_uniforms(view_proj, sim_tick, grid_params)

    // --- Collect Visible ---
    visible_tiles = collect_visible_tiles(tiling, camera, frustum)

    // --- Build Instance Buffers ---
    tile_instances.clear()
    belt_instances.clear()
    machine_instances.clear()
    item_instances.clear()

    for tile in visible_tiles:
        tile_instances.push(TileInstance from tile.transform)

        if let Some(cell) = world.get_cell(tile.address):
            for (grid_pos, entity_id) in cell.structures:
                match world.structures[entity_id]:
                    Belt(belt_id):
                        belt_instances.push(BeltInstance {
                            mobius: tile.transform,
                            grid_offset: grid_to_klein(grid_pos),
                            direction: belt.direction,
                            anim_phase: belt_anim_offset(sim_tick),
                        })
                        // Also push item instances for items on this belt segment
                        for item in belt_segment.visible_items():
                            item_instances.push(ItemInstance { ... })

                    Machine(machine_id):
                        machine_instances.push(MachineInstance {
                            mobius: tile.transform,
                            grid_offset: grid_to_klein(grid_pos),
                            machine_type: machine.type,
                            progress: machine.progress,
                        })

    // --- Upload ---
    tile_instances.upload(queue)
    belt_instances.upload(queue)
    machine_instances.upload(queue)
    item_instances.upload(queue)

    // --- Draw ---
    begin_render_pass(color_attachment, depth_attachment)

    // Pass 1: Tiles (1 draw call)
    set_pipeline(tile_pipeline)
    set_vertex_buffer(0, tile_mesh.vertices)
    set_vertex_buffer(1, tile_instances.buffer)
    set_index_buffer(tile_mesh.indices)
    draw_indexed(0..tile_mesh.num_indices, 0, 0..tile_instances.count)

    // Pass 2: Belts (1 draw call)
    set_pipeline(belt_pipeline)
    set_vertex_buffer(0, belt_mesh.vertices)
    set_vertex_buffer(1, belt_instances.buffer)
    set_index_buffer(belt_mesh.indices)
    draw_indexed(0..belt_mesh.num_indices, 0, 0..belt_instances.count)

    // Pass 3: Machines (1 draw call per type, or 1 with type in instance data)
    set_pipeline(machine_pipeline)
    set_vertex_buffer(0, machine_mesh.vertices)
    set_vertex_buffer(1, machine_instances.buffer)
    draw_indexed(0..machine_mesh.num_indices, 0, 0..machine_instances.count)

    // Pass 4: Items on belts (1 draw call)
    set_pipeline(item_pipeline)
    set_vertex_buffer(0, item_mesh.vertices)
    set_vertex_buffer(1, item_instances.buffer)
    draw_indexed(0..item_mesh.num_indices, 0, 0..item_instances.count)

    end_render_pass()

    // Pass 5: egui overlay (UI only, no game geometry)
    render_egui()

    submit()
```

---

## 5. System-by-System Design

### 5.1 Belt System

**Data model:** Belts occupy grid squares within tiles. Consecutive same-direction
belt segments within a tile merge into **transport lines**. Cross-tile belts connect
at tile edges.

```
Grid layout within a {4,5} tile:

  N (edge 0)
  +---------+
  |  64x64  |
W |  grid   | E
  |         |
  +---------+
  S (edge 2)

Grid coords: gx in [0, 63], gy in [0, 63]
  gx increases East, gy increases South
  (0,0) = NW corner, (63,63) = SE corner
```

**Transport line formation:**
1. When a belt is placed, check if it extends an existing line (same direction,
   adjacent grid square, same tile).
2. If yes, append to existing line (grow `gaps` array, increase `length`).
3. If no, create new single-segment line.
4. When a belt is removed, split its line into 0, 1, or 2 new lines.

**Cross-tile connections:**
A belt at the edge of a tile (gx=0, gx=63, gy=0, gy=63) facing outward connects
to the corresponding belt in the adjacent tile. The adjacent tile is found via
`neighbor_transforms[parity][direction]`. Edge mapping for {4,n}:
- gy=0 facing North -> neighbor 0, target gy=63
- gx=63 facing East -> neighbor 1, target gx=0
- gy=63 facing South -> neighbor 2, target gy=0
- gx=0 facing West -> neighbor 3, target gx=63

Cross-tile connections form `BeltEnd::Belt(other_line_id)` links.

**Item movement:** Fixed-point arithmetic. Speed in units of 1/256 grid squares
per tick. At 60 UPS, speed=4 means 4/256 = 1/64 squares/tick = 1 square/second.
Items are `ItemId` (1 byte). Gap values are `u16` (max 65535 = ~256 grid squares).

### 5.2 Machine System

**Hot/cold data split:**
- Hot (every tick): `progress`, `recipe_ticks`, `power_draw`, `state` — contiguous `Vec<T>`
- Cold (on interaction): `recipe`, `input_slots`, `output_slots`, `position` — separate `Vec<T>`
- Both indexed by `MachineId` (dense index 0..count)

**State machine:**

```
           place
  (empty) ------> Idle
                    |
           has      | no inputs
           inputs   v
            +--- NoInput
            |
            v
         Working ----(power ok)----> progress++
            |                          |
            | recipe_ticks == 0        |
            v                          |
         check output <----------------+
            |
       +----+----+
       |         |
   has room   full
       |         |
       v         v
     Idle    OutputFull ----(inserter drains)----> Idle
```

**Sleep/wake:** Machines in `Idle` with no adjacent active inserters skip the tick
entirely. A "dirty" flag wakes them when an inserter delivers items or an adjacent
belt changes state.

### 5.3 Inserter System

Inserters bridge belts and machines. Each inserter has:
- `source: EntityId` — what to grab from (belt endpoint or machine output)
- `dest: EntityId` — what to place into (belt input or machine input)
- `arm_progress: f32` — animation (0=source, 1=dest)
- `held_item: Option<ItemId>`

**Tick logic:**
1. If idle and source has item: grab, set state=Grabbing
2. If grabbing: advance arm toward source, when complete: take item, set state=Placing
3. If placing: advance arm toward dest, when complete: try insert, if dest full: state=Blocked
4. If blocked: retry each tick until dest accepts

**Transfer resolution:** Inserters read sources/dests in parallel (Phase 4a), then
apply transfers sequentially (Phase 4b). No two inserters share a source or dest
(enforced by placement rules), so parallel reads are safe.

### 5.4 Power System

Simple ratio-based model (like early Factorio):

1. Build connected components via BFS on power graph edges
2. For each component: `ratio = total_production / total_consumption`
3. Clamp ratio to [0, 1]
4. Every consumer in that component gets `power_draw = ratio`
5. Machines multiply their speed by `power_draw`

**Power nodes:** Quadrupoles produce power. Machines consume power. Dynamos are
advanced producers. Nodes connect to all other nodes within a configurable radius
(e.g., 8 grid squares). Power graph is rebuilt lazily when nodes are placed/removed.

### 5.5 Chunk System

**Chunk definition:** A chunk is all tiles sharing a common address prefix of
length `CHUNK_DEPTH`. For {4,5} with CHUNK_DEPTH=3, each chunk contains up to
~4^0 + 4^1 + 4^2 = 21 tiles (the subtree rooted at that prefix).

**Simulation scope:**

| Distance from player | Scope | Tick rate |
|----|---|---|
| 0-2 chunks | Active | Every tick |
| 3-4 chunks | Nearby | Every 4th tick |
| 5+ chunks | Frozen | No ticking; catch-up on approach |

**Catch-up on approach:**
When a Frozen chunk transitions to Active:
1. Compute `elapsed = current_tick - chunk.last_sim_tick`
2. For each transport line: `fast_forward(elapsed)` — adjust terminal gaps by `speed * elapsed`
3. For each machine: batch-complete recipes, fill output slots
4. Set `chunk.last_sim_tick = current_tick`

This is O(lines + machines) per chunk, not O(items). Gap-based belts make this trivial.

**Eviction:**
When `loaded.len() > max_loaded`, evict the chunk with the oldest `last_accessed`.
Before evicting, serialize its sim state (belt gaps, machine progress, inventories)
to a save buffer. On reload, deserialize and catch up.

---

## 6. Rendering Overhaul

### 6.1 Instance Buffer Architecture

Replace per-tile uniform buffer + dynamic offsets with instance buffers:

**Current:**
```
For each visible tile:
  queue.write_buffer(uniform_buf, offset, &uniforms)  // 256 bytes
  pass.set_bind_group(0, &bg, &[offset])
  pass.draw_indexed(...)                               // 1 draw call
// Total: N write_buffer + N set_bind_group + N draw_indexed
```

**Target:**
```
tile_instances.clear()
for each visible tile:
  tile_instances.push(TileInstance { ... })  // 32 bytes
tile_instances.upload(queue)                  // 1 write_buffer

pass.set_vertex_buffer(0, tile_mesh)
pass.set_vertex_buffer(1, tile_instances.buffer)
pass.draw_indexed(0..idx_count, 0, 0..tile_count)  // 1 draw call
// Total: 1 write_buffer + 1 draw_indexed
```

### 6.2 Shader Changes

**Global uniforms (bind group 0):** Shared across all pipelines.
```wgsl
struct Globals {
    view_proj: mat4x4<f32>,
    time: f32,
    grid_enabled: f32,
    grid_divisions: f32,
    klein_half_side: f32,
};
@group(0) @binding(0) var<uniform> globals: Globals;
```

**Per-instance data (vertex buffer slot 1):**
```wgsl
struct TileInstance {
    @location(5) mobius_a: vec2<f32>,
    @location(6) mobius_b: vec2<f32>,
    @location(7) depth: f32,
    @location(8) elevation: f32,
};

@vertex
fn vs_tile(vert: VertexInput, inst: TileInstance) -> VertexOutput {
    let z = apply_mobius(vert.pos_disk, inst.mobius_a, inst.mobius_b);
    let world = disk_to_bowl(z) + vec3(0.0, inst.elevation, 0.0);
    // ... normal, projection
}
```

**Belt shader:** Takes belt mesh (small rectangle) + BeltInstance. Positions the
rectangle on the tile surface using Mobius transform + Klein grid offset. Animates
arrows/chevrons using `anim_phase`.

**Item shader:** Billboard quad positioned on belt via Mobius + grid offset + lane
position. Texture lookup for item type.

### 6.3 Belt Mesh

Replace egui `convex_polygon()` overlay with a proper wgpu mesh:

```
Belt segment mesh (per grid square):
  - Small rectangle, ~1/64 of tile size
  - 4 vertices, 2 triangles
  - UV coords for directional texture (arrow pattern)

In vertex shader:
  1. Scale mesh to grid square size (1/grid_divisions of Klein half-side)
  2. Offset to grid position (grid_offset in Klein coords)
  3. Convert Klein -> Poincare: P = K / (1 + sqrt(1 - |K|^2))
  4. Apply Mobius transform (tile's transform)
  5. disk_to_bowl, project to clip space
```

This replaces ~4 egui convex_polygon calls per belt with 1 instance in a batch.

### 6.4 Draw Call Budget

| Pass | Draw calls | Instances |
|------|-----------|-----------|
| Tiles | 1 | up to 1024 |
| Grid overlay | 0 (done in tile shader) | — |
| Belts | 1 | up to 65536 |
| Machines | 1 per type (5 max) | up to 4096 |
| Items on belts | 1 | up to 65536 |
| egui UI | ~3-5 (egui internals) | — |
| **Total** | **~10** | — |

Down from current ~1024+ draw calls.

---

## 7. Dependencies & Conflicts

### 7.1 Implementation Dependencies

```
                    [Phase 1]
                  Fixed Timestep
                  & Game Loop
                       |
            +----------+----------+
            |                     |
       [Phase 2]            [Phase 3]
     Entity IDs &          Belt Simulation
     World Rewrite         (TransportLine)
            |                     |
            +----------+----------+
                       |
                  [Phase 4]
               Machine + Inserter
                  Simulation
                       |
                  [Phase 5]
                Power Network
                       |
                  [Phase 6]
              Instanced Rendering
              (can start earlier,
               but full benefit
               needs entity data)
                       |
                  [Phase 7]
               Chunk Streaming
                       |
                  [Phase 8]
                  Save / Load
```

### 7.2 Conflicts with Existing Code

| Existing code | Conflict | Resolution |
|---------------|----------|------------|
| `WorldState` (nested HashMap) | Replaced by typed pools + SlotMap | Phase 2: rewrite `world.rs`, update all callers in `app.rs` |
| `Structure { item, direction }` | Too simple for machines/belts | Phase 2: replace with `StructureKind` enum + per-type pools |
| `Vec<u8>` tile addresses | Heap alloc on every clone | Phase 2: switch to `SmallVec<[u8; 12]>` via type alias |
| Per-tile `Uniforms` + dynamic offsets | Replaced by instance buffers | Phase 6: new pipeline, new shader; old pipeline removed |
| Belt rendering via egui `convex_polygon` | Replaced by instanced belt mesh | Phase 6: delete egui belt overlay code in `app.rs:857-987` |
| `render_frame()` in `app.rs` (~300 lines) | Monolithic; needs splitting | Phase 6: extract into `RenderEngine` methods |
| `about_to_wait()` calls `process_movement` directly | Needs fixed timestep wrapper | Phase 1: wrap in accumulator loop |
| `camera_tile`, `camera_local`, `heading` on App | Should be `Camera` struct | Phase 1: extract `Camera` from `App` |
| `flash_screen_pos`, `belt_drag`, etc. on App | UI state mixed with game state | Phase 2: move to `UiState` struct |

### 7.3 Crate Dependencies to Add

| Crate | Purpose | Phase |
|-------|---------|-------|
| `slotmap` | Generational arena for entity IDs | 2 |
| `smallvec` | Inline tile addresses (no heap for depth <= 12) | 2 |
| `rayon` | Parallel iteration for sim phases | 4 |
| `serde` (already present) | Save/load serialization | 8 |
| `bincode` or `rmp-serde` | Binary serialization format | 8 |

### 7.4 Risk Areas

**Transport line merging across tile boundaries.** Belts in adjacent hyperbolic
tiles need to connect. The tile adjacency graph provides the mapping, but we need
to ensure:
- The edge-crossing direction is consistent (North of tile A = South of neighbor)
- Grid coordinates map correctly at boundaries (gx/gy flip or transpose depending on edge)
- Transport lines can span multiple tiles (or we use `BeltEnd::Belt` cross-links)

*Recommendation:* Start with per-tile transport lines connected by `BeltEnd::Belt`
cross-links. This avoids cross-tile line merging complexity. Optimize to multi-tile
lines later if profiling shows it matters.

**Shader rewrite scope.** The current shader does Mobius transform, bowl embedding,
finite-difference normals, Klein grid, and eldritch palette all in one. Splitting
into tile/belt/machine/item shaders means duplicating the Mobius+bowl math. Use
a shared WGSL include file (wgpu doesn't have `#include`, so use `naga_oil` or
string concatenation at build time).

**Camera extraction.** `App` currently owns camera state and couples it tightly
with tiling and input. Extracting `Camera` as a separate struct means passing
references between `Camera`, `TilingState`, and `InputState`. The borrow checker
will fight this if we're not careful — pass by-value snapshots where possible.

**Fixed timestep vs. existing input handling.** Currently, input events directly
modify game state in `window_event()`. With fixed timestep, input events should
be *queued* and consumed during the sim tick. `InputState` already buffers
`active_actions` and `just_pressed_actions`, so this mostly works — just need
to ensure `end_frame()` is called at the right time (after sim tick, not after
render frame).

---

## 8. Implementation Phases

### Phase 1: Fixed Timestep & Camera Extraction

**Goal:** Decouple simulation from rendering. Establish the tick/render boundary.

**Changes:**
- [ ] Extract `Camera` struct from `App` (fields: tile, local, heading, height, mode)
- [ ] Move `process_movement()`, `build_view_proj()`, `unproject_to_disk()` to `Camera`
- [ ] Add `GameLoop` struct with accumulator, `sim_tick` counter
- [ ] Wrap simulation in `about_to_wait()` with fixed timestep loop
- [ ] Interpolate camera position between ticks for smooth rendering
- [ ] Add `sim/mod.rs` and `sim/tick.rs` with `FixedTimestep` logic

**Files touched:** `app.rs` (major refactor), new `sim/tick.rs`, new `render/camera.rs`

**Validation:** Game feels identical to before. FPS and movement unchanged.
Add debug overlay showing UPS and FPS independently.

### Phase 2: World Rewrite & Entity IDs

**Goal:** Replace nested HashMaps with typed pools and stable entity IDs.

**Changes:**
- [ ] Add `slotmap` and `smallvec` to Cargo.toml
- [ ] Define `TileAddr = SmallVec<[u8; 12]>` type alias
- [ ] Replace `Vec<u8>` with `TileAddr` in `Tile`, `WorldState`, `CellState`, `BeltDrag`, etc.
- [ ] Define `EntityId` via `slotmap::SlotMap`
- [ ] Rewrite `WorldState`: `tile_grid` + `structures` SlotMap + `positions`/`directions` SecondaryMaps
- [ ] Define `StructureKind` enum: `Belt(BeltId)`, `Machine(MachineId)`, `PowerNode`, etc.
- [ ] Update all callers in `app.rs`: `try_place_at`, `handle_placement_click`, `handle_placement_drag`
- [ ] Update belt overlay rendering to use new world queries
- [ ] Extract `UiState` struct from `App` (flash, drag, cursor, panel open flags)

**Files touched:** `game/world.rs` (rewrite), `app.rs` (update callers), `Cargo.toml`

**Validation:** Belt placement and display still works. Belt drag still works. Inventory deduction still works.

### Phase 3: Belt Simulation

**Goal:** Items move on belts. The core factory gameplay loop begins.

**Changes:**
- [ ] Create `sim/belt.rs` with `TransportLine`, `BeltNetwork`, `BeltEnd`
- [ ] Implement gap-based item storage and `tick()` method
- [ ] Implement `fast_forward(elapsed_ticks)` for chunk catch-up
- [ ] Build transport line topology when belts are placed/removed
  - Same-direction consecutive segments within a tile merge into one line
  - Cross-tile connections via `BeltEnd::Belt` links
- [ ] Add belt input/output: items can be placed on belt input end, taken from output end
- [ ] Wire into simulation tick (Phase 5 of tick loop)
- [ ] Add debug visualization: show item positions on belts (text overlay or colored dots)
- [ ] Add item spawner (debug): place items on belt via click

**Files touched:** new `sim/belt.rs`, `sim/tick.rs`, `app.rs` (debug controls)

**Validation:** Place a line of belts. Spawn an item at one end. Watch it move to
the other end at the correct speed. Items compress when blocked. Items transfer
across tile boundaries.

### Phase 4: Machine & Inserter Simulation

**Goal:** Complete the production chain: belts -> inserters -> machines -> inserters -> belts.

**Changes:**
- [ ] Create `sim/machine.rs` with `MachinePool` (SoA hot/cold split)
- [ ] Implement machine state machine: Idle -> Working -> OutputFull / NoInput
- [ ] Create `sim/inserter.rs` with `InserterPool`
- [ ] Implement inserter grab/place logic with two-phase transfer
- [ ] Wire machines and inserters into simulation tick (Phases 3 and 4)
- [ ] Auto-create inserters when a machine is placed adjacent to a belt
  (or: make inserters a separate placeable item)
- [ ] Add machine UI: click machine to see recipe, progress, input/output slots
- [ ] Add recipe selection UI for placed machines

**Files touched:** new `sim/machine.rs`, new `sim/inserter.rs`, `sim/tick.rs`,
`app.rs` (machine UI), `ui/` (machine interaction panel)

**Validation:** Place a Composer machine. Place input belts with correct items.
Place output belt. Watch machine consume inputs, craft, and output to belt.

### Phase 5: Power Network

**Goal:** Machines require power. Quadrupoles supply it.

**Changes:**
- [ ] Create `sim/power.rs` with `PowerNetwork`
- [ ] Implement connected-component power solving
- [ ] Wire into simulation tick (Phase 2)
- [ ] Machines without power enter `NoPower` state
- [ ] Add power overlay visualization (satisfaction ratio as color)
- [ ] Add power info to machine UI

**Files touched:** new `sim/power.rs`, `sim/tick.rs`, `sim/machine.rs` (power check)

**Validation:** Machine with no quadrupole nearby: no crafting. Place quadrupole:
crafting resumes. Overload power (many machines, few quadrupoles): all slow down
proportionally.

### Phase 6: Instanced Rendering

**Goal:** Replace per-tile draw calls and egui belt overlay with instanced rendering.

**Changes:**
- [ ] Define instance buffer types: `TileInstance`, `BeltInstance`, `MachineInstance`, `ItemInstance`
- [ ] Create `InstanceBuffer<T>` helper (staging Vec + GPU buffer + upload)
- [ ] Create `RenderEngine` struct, extract rendering from `app.rs`
- [ ] Write `tile.wgsl` shader with per-instance Mobius from vertex buffer
- [ ] Write `belt.wgsl` shader: position rectangle on tile surface via instance data
- [ ] Write `machine.wgsl` shader: similar to belt but with machine-type-specific visuals
- [ ] Write `item.wgsl` shader: billboard or sprite at belt position
- [ ] Shared WGSL functions: `apply_mobius()`, `disk_to_bowl()`, `klein_to_poincare()`
  (use `naga_oil` for includes, or string concat)
- [ ] Build instance buffers each frame from visible tiles + world state
- [ ] Delete egui belt overlay code (`app.rs:857-987`)
- [ ] Delete per-tile uniform buffer and dynamic offset system

**Files touched:** `render/pipeline.rs` (rewrite), `render/instances.rs` (new),
`render/shader.wgsl` (split into multiple), `app.rs` (delete belt overlay, use RenderEngine)

**Validation:** Same visual output as before, but at ~10 draw calls instead of
~1024. Profile to confirm GPU time reduction. Belt rendering no longer depends on
egui.

### Phase 7: Chunk Streaming

**Goal:** Support unbounded world exploration without running out of memory.

**Changes:**
- [ ] Create `game/chunk.rs` with `ChunkManager`, `Chunk`, `ChunkAddr`
- [ ] Define chunk boundaries by address prefix depth
- [ ] Implement ring-based loading centered on player
- [ ] Implement LRU eviction with sim state serialization
- [ ] Implement freeze/thaw with fast-forward catch-up
- [ ] Integrate with `TilingState`: chunks request tile generation, `TilingState`
  generates tiles on demand instead of BFS-expanding globally
- [ ] Integrate with simulation: only tick Active/Nearby chunks

**Files touched:** new `game/chunk.rs`, `hyperbolic/tiling.rs` (on-demand generation),
`sim/tick.rs` (scoped simulation), `app.rs` (chunk manager updates)

**Validation:** Walk far from origin. Memory usage stays bounded. Return to a
previously visited factory — it has continued producing (via catch-up). No visual
pop-in at chunk boundaries.

### Phase 8: Save/Load

**Goal:** Persist world state across sessions.

**Changes:**
- [ ] Create `game/save.rs`
- [ ] Define save format: world state + chunk states + inventory + camera position
- [ ] Serialize with `bincode` or `rmp-serde`
- [ ] Auto-save on exit, manual save via UI
- [ ] Load on startup if save file exists
- [ ] Version the save format for forward compatibility

**Files touched:** new `game/save.rs`, `app.rs` (save/load triggers), `Cargo.toml`

**Validation:** Build a factory. Quit. Relaunch. Factory is intact and running.

---

## 9. Open Questions

### Gameplay

1. **Inserter model:** Are inserters explicit placeable entities (like Factorio) or
   implicit connections (like DSP's sorters)? Explicit inserters add strategic depth
   but more complexity.

2. **Belt lanes:** Single lane (like DSP) or dual lane (like Factorio)? Single
   lane is simpler and sufficient for v1.

3. **Underground/elevated belts:** Needed for routing in hyperbolic space where
   paths can't cross? Might be less necessary since hyperbolic space has "more room."

4. **Research/progression:** What unlocks what? The item tier system (0/1/2) suggests
   a tech tree but it's not defined yet.

5. **Win condition:** Is there one? Or is it sandbox?

### Technical

6. **WGSL includes:** `naga_oil` adds a dependency and build complexity. Alternative:
   concatenate shader strings at startup with `format!()`. Which do we prefer?

7. **Fixed-point precision for belts:** `u16` gaps with 1/256 grid square resolution
   gives max transport line length of 65535/256 = ~256 grid squares = 4 tiles.
   Enough? Or use `u32` for longer lines?

8. **Inserter placement rules:** One inserter per source-dest pair? Or allow
   multiple inserters on the same pair for throughput?

9. **Grid size:** Currently 64x64. Is this the right resolution? Smaller grids (32x32)
   mean fewer belt segments per tile but coarser placement. Larger grids (128x128)
   give finer control but more entities.

10. **Cross-tile transport lines:** Start with per-tile lines + cross-links (simpler),
    or go straight to multi-tile merged lines (faster simulation)?
    *Recommendation: per-tile + cross-links first.*
