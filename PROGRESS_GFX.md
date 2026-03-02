# Octofact — Procedural Machine Graphics

> Give each machine type a unique, abstract visual identity using SDF patterns,
> color gradients, shimmer, and animation — entirely in the fragment shader.
> No textures, no new meshes, no new bind groups.

---

## Phase G1: SDF Toolkit & Time Uniform

> Build the shared SDF library in common.wgsl and wire a time value into the
> shader globals so patterns can animate.

### Context

Currently `color_cycle` in `Globals` is a float used for tile palette cycling.
We need a monotonically increasing time value (seconds) available to all shaders
for animation. We can repurpose `color_cycle` as elapsed time (it already is in
practice — it's set from `Instant::now()` in the render loop) or add a dedicated
`time` field. Check what `color_cycle` is set to and whether renaming/reusing it
is safe.

The SDF toolkit goes in `common.wgsl` since all shaders are prefixed with it.
Keep it compact — only primitives we'll actually use across multiple machines.

### Tasks

- [x] Audit `color_cycle` usage — confirm it can serve as animation time or add a `time` field to `Globals`
- [x] Add SDF shape primitives to `common.wgsl`:
  - `sdf_circle(p, r)` — distance to circle boundary
  - `sdf_box(p, half_size)` — distance to axis-aligned rectangle
  - `sdf_segment(p, a, b)` — distance to line segment
  - `sdf_arc(p, r, angle_start, angle_span)` — distance to circular arc
- [x] Add SDF operations to `common.wgsl`:
  - `sdf_smooth_union(d1, d2, k)` — smooth minimum (soft blend)
  - `sdf_annular(d, thickness)` — turn any SDF into a ring/outline: `abs(d) - thickness`
- [x] Add rendering helpers to `common.wgsl`:
  - `sdf_fill(d)` — anti-aliased fill using `smoothstep` + `fwidth`
  - `sdf_stroke(d, width)` — anti-aliased stroke
  - `rot2(angle)` — 2D rotation matrix (`mat2x2<f32>`)
- [x] Add simple procedural noise to `common.wgsl`:
  - `hash21(p: vec2<f32>) -> f32` — pseudo-random hash
  - `vnoise(p: vec2<f32>) -> f32` — smooth value noise (bilinear interpolation of hashed grid)
- [x] Verify the build compiles and existing visuals are unchanged (no regressions)

### Design Notes

**SDF coordinate convention:** All machine pattern functions receive UV in `[0,1]×[0,1]`
over the top face. The SDF primitives should be centered at origin, so pattern functions
will do `let p = uv - 0.5;` to center. This matches the existing `port_indicators` UV space.

**`fwidth` for AA:** WGSL supports `fwidth()` which gives the screen-space derivative of
a value. `smoothstep(-fw, fw, d)` produces resolution-independent anti-aliased edges.
This is the standard technique for SDF rendering.

---

## Phase G2: 3D Topper Infrastructure + All Machine Shapes

> Ray-marched 3D shapes sitting on machine bases. A separate instanced draw call
> ("topper pass") with cube bounding-box geometry. Fragment shader ray marches
> per-machine-type 3D SDFs with proper lighting and perspective.

### Architecture

```
Existing:  machine pass  →  flat colored boxes with bevel, ports, state dimming
New:       topper pass   →  cube bounding volumes above each base, ray-marched 3D shapes

Render order: tiles → belts → machines (base) → toppers → items
```

Shared infrastructure: same `Globals` bind group (extended with `camera_world`),
same `MachineInstance` instance data (reused — no new instance type).

### Tasks

- [x] G2a: Extend `Globals` with `camera_world: vec4<f32>` (96→112 bytes) — Rust struct, all .wgsl Globals declarations, app.rs sets from camera height
- [x] G2b: Add 3D SDF primitives to `common.wgsl` — `sdf3_sphere`, `sdf3_box`, `sdf3_round_box`, `sdf3_torus`, `sdf3_octahedron`, `sdf3_cylinder`, `sdf3_capsule`, `sdf3_twist_y`, `rot3_x/y/z`
- [x] G2c: Add `TopperVertex` (3D cube vertex, 16 bytes) + `build_topper_mesh(n)` (subdivided cube, n=4) to `mesh.rs`
- [x] G2d: Add `TopperPipeline` to `pipeline.rs` — shares Globals bind group, back-face culling, depth write
- [x] G2e: Create `topper.wgsl` — vertex shader (positions cube in hyperbolic space via Klein→Poincare→Mobius→bowl), fragment shader (ray marching + normal estimation + directional lighting + fresnel rim)
- [x] G2f: Implement all 10 machine shapes in `machine_scene()` dispatcher:
  - Source (mt=5): Pulsing sphere
  - Composer (mt=0): Two counter-rotating tori
  - Inverter (mt=1): Tumbling octahedron
  - Embedder (mt=2): Rotating helix (spiral capsules)
  - Quotient (mt=3): Stella octangula (two interpenetrating octahedra)
  - Transformer (mt=4): Twisted torus
  - Quadrupole (mt=6): Pulsing elongated diamond
  - Dynamo (mt=7): Spinning cylinder with radial fins
  - Splitter (mt=8): Triangular prism
  - Storage (mt=9): Stacked cubes driven by fill level
- [x] G2g: Integrate into render engine — `TopperPipeline` + `InstanceBuffer<MachineInstance>` in `RenderEngine`, draw after machines, populate in same loop as machine instances

### Shape Assignments

| mt | Name | Size | Color | 3D Shape | Animation |
|---|---|---|---|---|---|
| 0 | Composer | 2×2 | blue | Stacked counter-rotating tori | Counter-rotate |
| 1 | Inverter | 3×3 | red | Octahedron | Slow tumble |
| 2 | Embedder | 3×3 | green | Helix (spiral capsules) | Axial rotation |
| 3 | Quotient | 3×3 | brown | Stella octangula (2 octahedra) | Counter-rotate halves |
| 4 | Transformer | 3×3 | purple | Twisted torus | Continuous twist |
| 5 | Source | 1×1 | lime | Pulsing sphere | Scale pulse |
| 6 | Quadrupole | 1×1 | gold | Elongated diamond | Gentle pulse |
| 7 | Dynamo | 2×2 | bright gold | Finned cylinder | Spin |
| 8 | Splitter | 1×1 | teal | Triangular prism | Color shimmer |
| 9 | Storage | 2×2 | amber | Stacked cubes | Fill-driven stack |

### State-Reactive Animation

- Working (`progress >= 0.0`): shapes animate at full speed via `globals.time`
- Idle (`progress == -1.0`): shapes frozen (anim_t = 0)
- No power (`progress == -2.0`): shapes frozen + desaturated + dimmed

---

## Phase G3: Animation & Polish

> Add life and depth to all topper shapes with additional visual effects.

### Tasks

- [ ] Add fresnel rim glow intensity tuning per machine type
- [ ] Add animated contour shimmer overlay on shapes
- [ ] Make crafting progress drive animation speed (acceleration/deceleration within craft cycle)
- [ ] Add soft shadow on machine base (darken center area based on topper shape)
- [ ] LOD: reduce march steps at distance (`disk_r > 0.75` → 24 steps; `disk_r > 0.85` → skip topper)
- [ ] Performance tuning: profile with 50+ machines, optimize step counts for simple shapes
- [ ] Final tuning pass: brightness, contrast, saturation across all 10 machine types

---

## Phase G4: Cross-Platform Native

> Switch from Metal-only to all primary backends so the game runs on
> Linux (Vulkan) and Windows (DX12/Vulkan) without code changes.

### Tasks

- [ ] Change `Backends::METAL` to `Backends::PRIMARY` in adapter request
- [ ] Audit for platform-specific assumptions (texture formats, surface configuration)
- [ ] Test compilation and verify no regressions
