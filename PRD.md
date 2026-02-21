# Octofact — Product Requirements Document

## Overview

Octofact is a real-time walking simulator on the **{8,3} hyperbolic tiling** (regular octagons, three meeting at each vertex) rendered via the Poincare disk model. Built in Rust with wgpu 28 targeting Apple Metal.

The player hovers above an infinite, procedurally-generated hyperbolic plane and navigates it in first person, watching the non-Euclidean geometry unfold beneath them.

---

## Core Requirements

### R1: Hyperbolic Tiling Engine

**R1.1 — Canonical cell addressing.**
Each cell in the {8,3} tiling has a unique canonical address encoded as a sequence of direction indices (0–7) along a shortest path from a distinguished origin. Example: `00002` = four steps in the same direction, then one step rotated two octagon-sides clockwise. Arbitrary paths must be reducible to canonical form so that the same cell always gets the same address regardless of how it was reached.

**R1.2 — Mobius arithmetic.**
Implement Mobius transformations on the Poincare disk for positioning tiles, composing motions, and rebasing the view. All geometry flows through `Complex` and `Mobius` types.

**R1.3 — Neighbor generation.**
Precompute the eight neighbor transforms for the canonical octagon (disk radius ~0.3646, from `cosh(R) = cos(pi/3)/sin(pi/8), r = tanh(R/2)`). Given any tile, produce its neighbors and their canonical addresses.

**R1.4 — BFS tiling with spatial dedup.**
Grow the visible tiling outward from the origin via breadth-first search. Deduplicate tiles by canonical address so each cell is stored and drawn exactly once. The tiling state must support incremental expansion as the player moves.

### R2: Rendering

**R2.1 — Poincare disk visualization.**
Render the tiling as a 3D scene: the disk is a surface in world space, the camera looks down at it. Each octagonal tile is a subdivided mesh (concentric rings for curvature fidelity).

**R2.2 — Vertex pipeline.**
The vertex shader applies a Mobius transformation (passed as uniform `a, b` coefficients) followed by hyperboloid embedding (disk coords to Y-up 3D via `disk_to_hyperboloid`), then standard view-projection.

**R2.3 — Fragment pipeline.**
The fragment shader applies a color palette driven by tile distance-from-origin (modulo a cycle length ~16), with surface lighting for depth cues. Target aesthetic: vivid, slightly eldritch.

**R2.4 — Mesh quality.**
Each octagon uses concentric-ring subdivision (4 rings, 3 segments per side) for smooth curvature representation. Vertex format is 32 bytes.

### R3: Camera & Movement

**R3.1 — First-person camera.**
Camera positioned above the disk surface, looking down. Maintains a `view_mobius` representing the player's position on the hyperbolic plane.

**R3.2 — WASD navigation.**
W/A/S/D keys translate the player across the hyperbolic plane. Movement is applied as Mobius composition, not Euclidean translation.

**R3.3 — Height control.**
Q/E keys raise and lower the camera above the disk surface.

**R3.4 — Per-frame rebase.**
Each frame, if the player has drifted far from the disk origin, rebase: recenter the Mobius view and re-expand the tiling around the new position. This keeps numerical precision stable.

### R4: World Model / Render Separation

**R4.1 — Decoupled state.**
The tiling state (cell addresses, neighbor graph, BFS frontier) is maintained independently from the rendering system. The renderer reads from the tiling state but does not own it.

**R4.2 — Incremental updates.**
As the player moves, the tiling state expands in the direction of travel. Tiles that fall far behind can be pruned (optional optimization).

### R5: Visual Polish

**R5.1 — Distance-based coloring.**
Each tile's color is determined by its canonical distance from the origin, mapped through a palette with a cycle length of ~16. The effect should make the exponential growth of hyperbolic space visually legible.

**R5.2 — Lighting.**
Basic surface lighting (normal-based diffuse + ambient) to give the curved surface depth.

### R6: Canonical Form Labels (Stretch)

**R6.1 — Toggle overlay.**
A keystroke toggles rendering of each visible tile's canonical address as text on the tile surface. This is a debug/educational feature, not required for the core experience.

---

## Architecture

```
src/
  main.rs                     # entry point
  app.rs                      # GpuState + App (ApplicationHandler), input, per-frame rebase
  hyperbolic/
    mod.rs
    poincare.rs               # Complex, Mobius, canonical_octagon, neighbor_transforms
    tiling.rs                 # Tile, TilingState (BFS + spatial dedup)
    embedding.rs              # disk_to_hyperboloid (Y-up)
  render/
    mod.rs
    mesh.rs                   # Vertex, concentric ring subdivision
    pipeline.rs               # Uniforms, RenderPipeline setup
    camera.rs                 # Camera with view_mobius, rebase
    shader.wgsl               # vertex: Mobius + hyperboloid; fragment: palette + lighting
```

## Tech Stack

| Component   | Choice          |
|-------------|-----------------|
| Language    | Rust            |
| GPU API     | wgpu 28         |
| Target GPU  | Apple Metal     |
| Windowing   | winit            |
| Math        | Hand-rolled (Mobius, Complex) |

## Non-Goals

- Multiplayer or networking.
- Tilings other than {8,3} (for now).
- Audio.
- Euclidean geometry fallback.

---

## Milestones

### M1 — Scaffold
Cargo project, wgpu 28 + winit boilerplate, blank window on Metal.

### M2 — Hyperbolic Math
`Complex`, `Mobius`, canonical octagon geometry, neighbor transforms, canonical address reduction. Unit tests for all math.

### M3 — Tiling State
BFS tile generation from origin, spatial dedup by canonical address, incremental expansion.

### M4 — Static Render
Render the tiling as a Poincare disk viewed from above. Subdivided octagon meshes, Mobius + hyperboloid vertex pipeline, basic coloring.

### M5 — Camera & Movement
WASD hyperbolic movement, Q/E height, per-frame rebase, smooth navigation.

### M6 — Visual Polish
Distance-based palette, lighting, eldritch aesthetic tuning.

### M7 — Labels (Stretch)
Toggleable canonical-form text overlay on tiles.
