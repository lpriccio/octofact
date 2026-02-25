# Octofact — Product Requirements Document

## Overview

Octofact is a factory-automation game on the **{4,n} hyperbolic tiling** (regular squares, n meeting at each vertex, default n=5). Built in Rust with wgpu 28 targeting Apple Metal. The player builds production chains on an infinite hyperbolic plane, extracting mathematical resources, processing them through tiered machines, and pushing outward into exponentially expanding space.

The rendering prototype (Poincare disk visualization, camera, movement, cell labels) is complete. This PRD covers the full factory game built on top of it.

---

## Core Requirements

### R1: Hyperbolic Tiling Engine

**R1.1 — Canonical cell addressing.**
Each cell in the {4,n} tiling has a unique canonical address encoded as a sequence of direction indices (0–3) along a shortest path from a distinguished origin. Arbitrary paths must be reducible to canonical form so that the same cell always gets the same address regardless of how it was reached.

**R1.2 — Mobius arithmetic.**
Implement Mobius transformations on the Poincare disk for positioning tiles, composing motions, and rebasing the view. All geometry flows through `Complex` and `Mobius` types.

**R1.3 — Neighbor generation.**
Precompute the four neighbor transforms for the canonical square (disk circumradius ~0.3846 for {4,5}). Given any tile, produce its neighbors and their canonical addresses.

**R1.4 — BFS tiling with spatial dedup.**
Grow the visible tiling outward from the origin via breadth-first search. Deduplicate tiles by canonical address so each cell is stored and drawn exactly once. The tiling state must support incremental expansion as the player moves.

### R2: Rendering

**R2.1 — Poincare disk visualization.**
Render the tiling as a 3D scene: the disk is a surface in world space, the camera looks down at it. Each square tile is a subdivided mesh (concentric rings for curvature fidelity).

**R2.2 — Vertex pipeline.**
The vertex shader applies a Mobius transformation (passed as per-instance data) followed by hyperboloid embedding (disk coords to Y-up 3D via `disk_to_hyperboloid`), then standard view-projection.

**R2.3 — Fragment pipeline.**
The fragment shader applies a color palette driven by tile distance-from-origin (modulo a cycle length ~16), with surface lighting for depth cues. Target aesthetic: vivid, slightly eldritch.

**R2.4 — Mesh quality.**
Each square uses concentric-ring subdivision (8 rings, 4 segments per side) for smooth curvature representation. Vertex format is 32 bytes.

**R2.5 — Instanced rendering.**
One draw call per entity type per frame. Per-instance data (Mobius transform, direction, animation phase) in vertex buffers. Target ~10 draw calls total, down from ~1024.

### R3: Camera & Movement

**R3.1 — First-person camera.**
Camera positioned above the disk surface, looking down. Maintains a `view_mobius` representing the player's position on the hyperbolic plane.

**R3.2 — WASD navigation.**
W/A/S/D keys translate the player across the hyperbolic plane. Movement is applied as Mobius composition, not Euclidean translation.

**R3.3 — Height control.**
Q/E keys raise and lower the camera above the disk surface.

**R3.4 — Per-frame rebase.**
Each frame, if the player has drifted far from the disk origin, rebase: recenter the Mobius view and re-expand the tiling around the new position. This keeps numerical precision stable.

### R4: Simulation Engine

**R4.1 — Fixed timestep.**
60 UPS independent of FPS. Accumulator pattern with interpolation for camera only.

**R4.2 — Belt simulation.**
Gap-based transport line model (Factorio-style). O(1) per unblocked line per tick. Items advance on belts each tick. Cross-tile connections via belt-end links.

**R4.3 — Machine simulation.**
Machines consume inputs, craft over time, produce outputs. State machine: Idle → Working → OutputFull / NoPower / NoInput. Hot/cold data split for cache efficiency.

**R4.4 — Inserter simulation.**
Inserters bridge belts and machines, transferring items between them. Two-phase tick: parallel plan, sequential apply.

**R4.5 — Power network.**
Ratio-based power model. Connected components via BFS on power graph. Satisfaction ratio determines machine speed.

### R5: World Model

**R5.1 — Typed entity pools.**
SlotMap-based entity IDs with dense Vec<T> per entity type. No full ECS — ~6 known entity types with fixed components.

**R5.2 — Cell-relative coordinates.**
Every entity is `(TileAddr, grid_x, grid_y)`. Never global Poincare/Klein coords in game state. Each cell contains a 64x64 Euclidean internal grid.

**R5.3 — Chunk streaming.**
Address-prefix chunks. Ring loading around player. LRU eviction. Freeze distant chunks; fast-forward on approach.

**R5.4 — Save/load.**
Persist discovered cells, structures, belt contents, inventory, camera position. Undiscovered cells generated on first visit from deterministic seed.

### R6: Factory Gameplay

**R6.1 — Resources & recipes.**
26 items, 23 recipes, 5 machine types (Composer, Inverter, Embedder, Quotient, Transformer). Two-tier production chains. See ITEMS.md and GAME.md.

**R6.2 — Transport.**
Belts (backbone), tunnel belts (short-range underground), pipes (fluids), trains (long-distance bulk). All physical, all curve through hyperbolic space.

**R6.3 — Power.**
Dynamos generate power, Quadrupoles transmit it. Power infrastructure competes for Composer time and Null Set supply.

**R6.4 — Research.**
Knowledge Sheaves consume Axiomatic Science to advance the tech tree. Unlocks T2 machines, higher-tier recipes, infrastructure upgrades.

**R6.5 — Milestone-based progression.**
No hard win condition. Escalating milestones give structure: first Line Segment, first self-built Composer, first Extraction Beacon, reach depth 20, etc.

### R7: UI (egui)

**R7.1 — Screen-space UI.**
All in-game windows rendered via egui (`egui-wgpu` + `egui-winit`). Build selector, inventory, tech tree, settings, milestone log.

**R7.2 — Input capture.**
egui consumes keyboard/mouse when UI is focused. WASD suppressed while UI panels are open. Simulation continues except in settings menu.

**R7.3 — Canonical form labels.**
Toggle overlay rendering each visible tile's canonical address as text on the tile surface (glyphon pipeline). Debug/educational feature.

---

## Architecture (target)

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
    config.rs                   # Settings persistence (TOML)
    input.rs                    # Action-based input layer
    items.rs                    # Item definitions
    inventory.rs                # Player inventory
    recipes.rs                  # Recipe definitions
    world.rs                    # EntityId, TileAddr, typed pools
    chunk.rs                    # ChunkManager, streaming, freeze/thaw
    save.rs                     # Serialization
  hyperbolic/
    mod.rs
    poincare.rs                 # Complex, Mobius, canonical_polygon, neighbor_transforms
    tiling.rs                   # Tile, TilingState (BFS + spatial dedup)
    embedding.rs                # disk_to_hyperboloid (Y-up)
  render/
    mod.rs
    mesh.rs                     # Vertex (32 bytes), concentric ring subdivision
    pipeline.rs                 # Instanced pipelines per entity type
    camera.rs                   # Camera with view_mobius, rebase
    instances.rs                # Instance buffer management, upload
    shader.wgsl                 # Split: tile.wgsl, belt.wgsl, machine.wgsl, item.wgsl
  ui/
    (egui windows: build selector, inventory, tech tree, settings, etc.)
```

## Tech Stack

| Component   | Choice          |
|-------------|-----------------|
| Language    | Rust            |
| GPU API     | wgpu 28         |
| Target GPU  | Apple Metal     |
| Windowing   | winit 0.30      |
| UI          | egui + egui-wgpu + egui-winit |
| Text        | glyphon 0.10 (cosmic-text 0.15) |
| Math        | Hand-rolled (Mobius, Complex) |
| Entity IDs  | slotmap         |
| Addresses   | smallvec        |
| Parallelism | rayon           |

## Non-Goals

- Multiplayer or networking.
- Tilings other than {4,n}.
- 3D stacking, bridges, or vertical gameplay.
- Long-range teleportation or abstracted logistics.

---

## Milestones

### Prototype (complete)

| Phase | Description | Status |
|-------|-------------|--------|
| P1 | Scaffold + blank window | Done |
| P2 | Hyperbolic math (Complex, Mobius) | Done |
| P3 | Tiling state (BFS + embedding) | Done |
| P4 | Basic rendering pipeline | Done |
| P5 | Cell labels (glyphon 0.10) | Done |
| P6 | Camera & WASD movement | Done |
| P7 | Visual polish | Done |

### Factory Game (see GAME_PLAN.md for details)

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Fixed timestep & camera extraction | Pending |
| 2 | World rewrite & entity IDs | Pending |
| 3 | Belt simulation | Pending |
| 4 | Machine & inserter simulation | Pending |
| 5 | Power network | Pending |
| 6 | Instanced rendering | Pending |
| 7 | Chunk streaming | Pending |
| 8 | Save/load | Pending |
