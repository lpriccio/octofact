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

- [ ] Audit `color_cycle` usage — confirm it can serve as animation time or add a `time` field to `Globals`
- [ ] Add SDF shape primitives to `common.wgsl`:
  - `sdf_circle(p, r)` — distance to circle boundary
  - `sdf_box(p, half_size)` — distance to axis-aligned rectangle
  - `sdf_segment(p, a, b)` — distance to line segment
  - `sdf_arc(p, r, angle_start, angle_span)` — distance to circular arc
- [ ] Add SDF operations to `common.wgsl`:
  - `sdf_smooth_union(d1, d2, k)` — smooth minimum (soft blend)
  - `sdf_annular(d, thickness)` — turn any SDF into a ring/outline: `abs(d) - thickness`
- [ ] Add rendering helpers to `common.wgsl`:
  - `sdf_fill(d)` — anti-aliased fill using `smoothstep` + `fwidth`
  - `sdf_stroke(d, width)` — anti-aliased stroke
  - `rot2(angle)` — 2D rotation matrix (`mat2x2<f32>`)
- [ ] Add simple procedural noise to `common.wgsl`:
  - `hash21(p: vec2<f32>) -> f32` — pseudo-random hash
  - `vnoise(p: vec2<f32>) -> f32` — smooth value noise (bilinear interpolation of hashed grid)
- [ ] Verify the build compiles and existing visuals are unchanged (no regressions)

### Design Notes

**SDF coordinate convention:** All machine pattern functions receive UV in `[0,1]×[0,1]`
over the top face. The SDF primitives should be centered at origin, so pattern functions
will do `let p = uv - 0.5;` to center. This matches the existing `port_indicators` UV space.

**`fwidth` for AA:** WGSL supports `fwidth()` which gives the screen-space derivative of
a value. `smoothstep(-fw, fw, d)` produces resolution-independent anti-aliased edges.
This is the standard technique for SDF rendering.

---

## Phase G2: Source & Composer Patterns

> First two machine patterns. Source is 1×1 and simple (good for validating the
> approach). Composer is 2×2 and the most common machine (good for stress-testing
> visual density). Get user feedback before proceeding to remaining machines.

### Context

**Source** (1×1, lime, `machine_type=5`): Emits items from nothing. Visual metaphor:
origin, emanation, wellspring. Pattern idea: concentric rings radiating outward,
pulsing with time. Bright center fading to dark edges.

**Composer** (2×2, blue, `machine_type=0`): Combines inputs into composite structures.
The workhorse T1 machine. Visual metaphor: convergence, fusion, composition.
Pattern idea: two or more arc shapes flowing inward and merging at center.
Input side (south) has flowing-in arcs, output side (north) has a unified shape.

### Tasks

- [ ] Add `machine_pattern(uv, mt, progress, time) -> vec4<f32>` dispatcher function to `machine.wgsl` — returns `(rgb_color_offset, emissive_intensity)` or `vec4(0)` for unimplemented types
- [ ] Implement Source pattern: concentric expanding rings with lime→dark radial gradient, ring phase animated by time, ring brightness modulated by crafting progress (brighter when producing)
- [ ] Implement Composer pattern: curved arcs flowing from south edge toward center, merging into a central circular motif; blue gradient with lighter intersection highlights; arcs animate (flow inward) when `progress >= 0.0`
- [ ] Integrate `machine_pattern` into `fs_machine`: blend pattern output with existing base color and lighting — pattern should enhance, not replace, the existing bevel/port rendering
- [ ] Verify both patterns look correct at different zoom levels and facing rotations
- [ ] Check performance: no visible frame rate change with 50+ machines on screen

---

## Phase G3: Inverter, Embedder, Quotient & Transformer

> The four production machines. Each gets a unique SDF composition reflecting
> its mathematical operation.

### Context

These are the machines players stare at most — production lines are built from them.
Each should be instantly recognizable at a glance, even at medium zoom where the
machine is ~30px on screen. The SDF patterns should be bold enough to read at that
size but have finer detail that rewards zooming in.

**Inverter** (3×3, red, `machine_type=1`): Reverses a mapping. Visual metaphor:
reflection, inversion, inside-out. Pattern: nested circle-in-square where the inner
region inverts the brightness/hue field. A moiré-like interference pattern at the
boundary. Rotational symmetry with a twist — the inner region is "flipped."

**Embedder** (3×3, green, `machine_type=2`): Maps one object into the structure of
another. Two inputs, one output. Visual metaphor: overlapping, nesting, containment.
Pattern: three overlapping smooth lobes (trefoil/Venn shape), with highlights at
intersection regions. The two input sides have distinct lobe orientations; the output
side shows the merged form.

**Quotient** (3×3, brown, `machine_type=3`): Divides one structure by another, producing
quotient and remainder. Visual metaphor: division, asymmetry, separation.
Pattern: a smooth S-curve or diagonal dividing the surface into two distinct regions —
one dense with fine concentric detail, the other sparse with a single clean shape.
Two outputs, so the visual shows "two things emerging from one."

**Transformer** (3×3, purple, `machine_type=4`): Applies a transformation across
multiple inputs simultaneously. Visual metaphor: rotation, morphing, transmutation.
Pattern: a smooth gear or turbine shape with rounded teeth, slowly rotating via time
uniform. Multiple input/output flow channels visible as radial grooves.

### Tasks

- [ ] Implement Inverter pattern: nested inversion motif — outer region and inner circle with complementary color/brightness fields; boundary ring with interference fringe
- [ ] Implement Embedder pattern: three smooth overlapping lobes (trefoil), intersection regions highlighted, lobes oriented toward input/output sides
- [ ] Implement Quotient pattern: smooth dividing curve splitting surface into dense/sparse halves; each half has distinct sub-pattern
- [ ] Implement Transformer pattern: rounded gear/turbine shape, animated rotation via time, radial flow channels
- [ ] Verify all patterns respect facing rotation (patterns should rotate with the machine's facing direction)

---

## Phase G4: Infrastructure Buildings

> Quadrupole, Dynamo, Splitter, and Storage. These are utility/infrastructure
> buildings, visually distinct from production machines.

### Context

Infrastructure buildings should share a visual language that distinguishes them from
production machines — slightly different style, maybe more geometric/regular vs the
organic curves of production machines.

**Quadrupole** (1×1, gold, `machine_type=6`): Power relay. Transmits power but
doesn't generate it. Visual metaphor: field lines, transmission, conductance.
Pattern: four-fold symmetric field line pattern — curves emanating from four poles
and connecting to adjacent space. Subtle pulse when power is flowing.

**Dynamo** (2×2, bright gold, `machine_type=7`): Power generator. Visual metaphor:
vortex, energy concentration, rotation. Pattern: Archimedean spiral from center,
bright gold core fading outward, spiral animated (slowly rotating). More energetic/
intense than Quadrupole.

**Splitter** (1×1, teal, `machine_type=8`): Splits/merges belt flow. Visual metaphor:
branching, divergence, flow control. Pattern: smooth channel shapes that match the
splitter's actual connected directions (encoded in the `progress` bitmask). Channels
glow where flow is active.

**Storage** (2×2, amber, `machine_type=9`): Holds items. Visual metaphor: containment,
capacity, accumulation. Pattern: horizontal bands that fill from bottom based on the
`progress` field (0.0 = empty, 1.0 = full). Bands have slight curvature, glow brighter
as storage fills. Container outline with rounded corners.

### Tasks

- [ ] Implement Quadrupole pattern: four-fold symmetric field lines, animated pulse when power flows (`power_sat > 0`)
- [ ] Implement Dynamo pattern: Archimedean spiral, animated rotation, bright gold core with radial falloff
- [ ] Implement Splitter pattern: directional flow channels decoded from progress bitmask, glowing active channels
- [ ] Implement Storage pattern: curved horizontal fill bars driven by progress value, container outline, fill-level glow

---

## Phase G5: Shimmer & Animation Polish

> Add life and depth to all machine patterns with shimmer effects and
> state-reactive animation. This pass refines all existing patterns.

### Context

After G2–G4, every machine has a unique static (or simply animated) SDF pattern.
This phase adds the "juice" — subtle effects that make machines feel alive and
convey state at a glance. These effects layer on top of existing patterns.

### Tasks

- [ ] Add fresnel glow to machine top faces: brighter edges where the bowl surface normal diverges from the view direction — pass view direction to fragment shader (add to `VertexOutput`, compute in vertex shader)
- [ ] Add animated contour shimmer: `sin(sdf_distance * frequency + time * speed)` overlay on all patterns — creates pulsing topographic-map-like contour lines
- [ ] Make crafting progress drive pattern animation speed, not just brightness: working machines have faster shimmer/animation, idle machines are static, unpowered machines are desaturated and frozen
- [ ] Add subtle value noise overlay to all machine surfaces: slow-scrolling low-frequency noise that creates a "living surface" feel, mixed at ~10% opacity
- [ ] Final tuning pass: adjust brightness, contrast, saturation, and animation speeds across all 10 machine types so they form a cohesive visual family while remaining individually distinct

### Design Notes

**Fresnel glow** needs the view direction in the fragment shader. Currently `VertexOutput`
doesn't include it. Add `world_pos` or `view_dir` to the output struct, compute in
`vs_machine`. The fresnel term is `pow(1.0 - max(dot(normal, view_dir), 0.0), exponent)`.

**State-reactive animation:** Currently `progress >= 0.0` means working (0–1 cycle),
`progress == -1.0` means idle, `progress == -2.0` means no power. The pattern
animation time should be: `time * speed` when working, `frozen_time` when idle,
`frozen_time` with desaturation when unpowered. The speed can scale with `progress`
within a craft cycle for a nice acceleration/deceleration feel.

---

## Phase G6: Cross-Platform Native

> Switch from Metal-only to all primary backends so the game runs on
> Linux (Vulkan) and Windows (DX12/Vulkan) without code changes.

### Context

Currently `Backends::METAL` is hardcoded. wgpu abstracts over Vulkan, DX12, and Metal
via the naga shader compiler (WGSL → SPIR-V / DXIL / MSL). Since we use no
Metal-specific APIs, switching to `Backends::PRIMARY` should be straightforward.
The main risks are platform-specific wgpu backend bugs and any assumptions about
texture formats or feature support.

### Tasks

- [ ] Change `Backends::METAL` to `Backends::PRIMARY` in adapter request
- [ ] Audit for any platform-specific assumptions: texture formats, surface configuration, feature flags
- [ ] Replace `pollster::block_on` with a pattern that works on all platforms (it should already — `pollster` is cross-platform for native, just not for web)
- [ ] Test compilation on the host platform (macOS/Metal) — verify no regressions
- [ ] Document any platform-specific notes in CLAUDE.md if discovered

### Design Notes

**What might surface:** Different backends may have different default surface texture
formats (e.g., `Bgra8UnormSrgb` on Metal vs `Rgba8UnormSrgb` on Vulkan). The code
should use `surface.get_capabilities(adapter).formats[0]` or similar rather than
hardcoding a format. Check the surface configuration code.

---

## Dependencies

```
Phase G1 (SDF toolkit)     ─── no dependencies, start here
Phase G2 (Source+Composer)  ─── depends on G1
Phase G3 (production)       ─── depends on G1 (not G2 — patterns are independent)
Phase G4 (infrastructure)   ─── depends on G1 (not G2/G3)
Phase G5 (shimmer)          ─── depends on G2, G3, G4 (all patterns must exist)
Phase G6 (cross-platform)   ─── no dependencies, can be done at any time
```

G2, G3, and G4 can be done in any order after G1, but G2 first is recommended
to validate the approach before investing in all 10 machine types.
