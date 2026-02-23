# Octofact — Graphics Implementation Notes

Technical details for the rendering pipeline. See [GAME.md](GAME.md) for the design context.

---

## Asset Pipeline Overview

Two phases, both code-only. No external modeling tools.

**Phase 1: Procedural geometry.** All structures are Rust functions that emit `Vec<Vertex>` + `Vec<u16>`, extending the existing `build_polygon_mesh` pattern in `src/render/mesh.rs`. The 32-byte `Vertex` format (position, normal, uv) stays as-is. Every shape is parametric code.

**Phase 2: SDF raymarching.** A second render pass for showcase impossible objects (Klein Bottles, Monopole fields, White Hole singularities, Boltzmann Node flicker). Small on screen, bounded in count. SDF lighting must match the rasterized pass.

---

## Conveyor Belts

### Segment Templates

Pre-bake a small set of belt mesh templates at startup. Each is a procedural function returning `(Vec<Vertex>, Vec<u16>)`:

| Template | Shape | Notes |
|----------|-------|-------|
| Straight | Flat ribbon with raised edges, 1 grid square long | The workhorse. |
| Curve | Quarter-arc ribbon, 90-degree turn | Left and right are the same mesh, mirrored via instance transform. |
| Entrance | Belt tapering into a building face | Visual cue that the belt connects directly to a structure input. |
| Exit | Belt emerging from a building face | Matching output visual. |

Each template is a shallow trough or channel — geometric, clean, a handful of triangles. No organic shapes, no rounded bevels. The belt *is* the groove in the grid. Belts connect directly into buildings (Satisfactory-style, no inserters) — the entrance/exit templates make the connection point visually explicit.

**Splitters** are separate 2x2 structures, not belt templates. They have their own procedural mesh: a flat junction box with 4 ports, each showing directional arrows indicating input/output configuration. The arrows update when the player reconfigures the splitter mode (1/3 split, 3/1 merge, 2/2 balance). Rendered as a single instanced mesh type with per-instance data encoding the port configuration.

**Tunnel belts** are a matched pair of portal-frame meshes (entrance + exit) with no visible geometry between them. Each portal is a 1x1 procedural mesh: a rectangular frame with an inset face. The inset face gets a shimmer effect in the fragment shader — a scrolling distortion pattern (UV-based noise, time-driven) that implies items are phasing through the metric. The shimmer color tints toward the paired exit's direction to help the player visually trace which entrance connects to which exit. Items approaching a tunnel entrance visually shrink/fade over the last grid square; items emerging from an exit expand/fade in. No underground geometry is rendered — the tunnel is purely logical.

### Instanced Rendering

Thousands of belt segments may be visible simultaneously. One draw call per template type, not one per segment.

- **Instance buffer** — per-segment data: grid position, rotation (0/90/180/270), animation phase offset, belt speed. Laid out as a second vertex buffer with `step_mode: VertexStepMode::Instance`.
- **Template meshes** stored in a shared vertex/index buffer. Each template type is a draw call with its own index range + instance range.
- Fits directly into the existing wgpu pipeline. `Vertex::desc()` stays unchanged; add a second `VertexBufferLayout` for instance attributes.

### Belt Animation

The belt surface scrolls via a time-based UV offset in the vertex shader. No geometry changes per frame.

```
// In shader: scroll belt texture along belt direction
let scroll = u.time * belt_speed + instance.phase_offset;
let animated_uv = vec2<f32>(in.uv.x + scroll, in.uv.y);
```

The groove pattern on the belt surface slides along, implying motion. Direction comes from the per-instance rotation. Essentially free — one uniform update per frame.

### Items on Belts

Klein Bottles riding belts are separate mesh instances positioned along the belt path.

- **Simulation:** Items advance one grid square per tick (fixed-tick simulation). Position is a belt segment index.
- **Rendering:** Between ticks, lerp item position along the belt path for visual smoothness. This is a render concern, not a simulation concern — the authoritative position is always the discrete tick state.
- **Phase 1:** Klein Bottles are small procedural meshes (twisted torus-ish). Instanced like belt segments.
- **Phase 2:** Klein Bottles graduate to SDF raymarched objects — non-orientable surface rendered correctly, catching the light wrong.

### Hyperbolic Curvature

Within a cell, belts live on a flat 128x128 grid — standard factory game rendering. The existing vertex shader already applies the Mobius transform to all geometry in Poincare disk coordinates, so belt meshes defined in local cell coordinates warp correctly into the disk automatically. No special handling needed.

Belts near the disk boundary will visibly compress and curve. This is correct behavior and visually striking — the curvature of the belt reveals the curvature of the space. Long belt runs across multiple cells will show the geodesic bending that makes hyperbolic logistics hard.

### Cell Boundary Crossings

When a belt exits one cell and enters a neighbor, the two segments are in different local coordinate systems related by the neighbor's Mobius transform. Visually, the shader handles this — each cell's geometry is transformed independently and they meet at the boundary. The seam should be imperceptible if the belt endpoints are snapped to the shared cell edge (128 grid units wide, so alignment is at discrete grid positions along the edge).

**Cell corners are exclusion zones.** Building is banned in a small region around each cell corner (a few grid squares). At a vertex of the {4, n} tiling, n cells meet — the geometry is ambiguous and there's no clean way to assign corner grid squares to a single cell. Transport crosses cell boundaries only along edges, never through corners. Visually, the exclusion zone can be rendered as darkened or cracked ground — the curvature concentrates at these points, and the Surface shows the strain.

---

## Pipes (Future)

Same instanced-template approach as belts. Pipe templates are cylindrical rather than trough-shaped. Can layer under belts on the same grid square (offset slightly in Y). Pump stations are larger procedural meshes at regular intervals.

## Rail (Future)

Rail templates are parallel-bar segments (two raised rails + cross-ties). Wider than belts. Trains are multi-segment procedural meshes that advance along rail paths. Train rendering is the same lerp-between-ticks approach as belt items, but the train mesh spans multiple grid squares and follows the rail spline.

## Structures (Future)

Each structure type gets a procedural mesh generator:

| Structure | Geometry sketch |
|-----------|----------------|
| Miner (3x3) | Low cylinder with radial drill bit, rotating |
| Composer (3x3) | Interlocking rings that rotate to "compose" — input face pulls items in, output face emits product |
| Inverter (1x1) | Small mirrored prism. Items enter one face, the inverse exits the opposite. |
| Embedder (2x2) | Two input funnels converging into a single output channel. Nested-shape motif. |
| Quotient (2x4) | Elongated divider. Two inputs on one end, two outputs on the other. Bisected geometry. |
| Transformer (6x3) | Wide tri-channel machine. Three parallel input/output lanes. Rotating transformation matrices as visual motif. |
| Knowledge Sheaf (5x5) | Layered disc structure, pages fanning open. Consumes Axiomatic Science, glows when researching. |
| Quadrupole (3x3) | Four-pronged pole structure. Visible power field lines radiating outward. |
| Dynamo (5x5) | Rotating core suspended between two Quadrupole-like armatures. Hums. |
| Train Station (8x4) | Beveled rectangular platform with rail slots |
| Extraction Beacon (21x21) | Tall prism with internal glow, phase 2 SDF core |

---

## Screen-Space UI (egui)

All in-game windows — build selector, inventory, tech tree, settings, milestone log — are rendered via egui. In-world text (cell labels) stays on the existing glyphon pipeline. The two systems coexist: glyphon renders into the 3D scene, egui renders a 2D overlay on top.

### Integration

**Crates:** `egui`, `egui-wgpu`, `egui-winit`.

**Render pass:** egui runs as the final render pass each frame, after the tile/structure pass and after glyphon labels. `egui-wgpu` provides a `Renderer` that takes egui's paint output and draws it into the swapchain surface. This is additive — it doesn't touch the existing pipeline.

**Input:** `egui-winit` converts winit `WindowEvent`s into egui input. Call `egui_winit::State::on_window_event()` before the game's own input handling. When egui reports `wants_keyboard_input` or `wants_pointer_input`, suppress those events from reaching the game — this prevents WASD movement while typing in a text field, or clicks on a UI panel from placing structures in the world.

**Frame loop sketch:**

```
// In App::about_to_wait or equivalent:
let raw_input = egui_state.take_egui_input(&window);
let full_output = egui_ctx.run(raw_input, |ctx| {
    // Draw all active windows:
    build_selector(ctx, &game_state);
    inventory_window(ctx, &game_state);
    tech_tree(ctx, &game_state);
    settings_menu(ctx, &mut config);
    // ...
});
// Handle egui output (clipboard, cursor, etc.)
egui_state.handle_platform_output(&window, full_output.platform_output);
// Render egui
egui_renderer.paint(&device, &queue, &mut encoder, &view, &full_output);
```

Each window is a function that takes `&egui::Context` and the relevant game/config state. Immediate mode: the function runs every frame, emitting widgets conditionally based on whether the window is open. No persistent widget objects to manage.

### Styling

Override `egui::Visuals` to match the aesthetic:

- **Dark background** — near-black panels with slight transparency so the game world shows through.
- **Monospace font** — load a monospace face into egui's font system. All UI text is monospace, matching the canonical-address labels.
- **Sharp corners** — set `rounding` to 0 or near-0 on all widgets. No rounded buttons, no soft edges.
- **Muted accent color** — a single cool-toned highlight (the rim purple from the tile shader, `rgb(102, 77, 179)` or similar) for selected items, active buttons, progress bars.
- **Minimal chrome** — no title bar decorations, no window shadows. Windows are flat rectangles with a thin border.

The goal is UI that feels like it's rendered by the same machine that runs the factory — geometric, precise, no ornamentation.

### Input Capture

When any egui window is open and has focus:
- Keyboard events go to egui, not the game. WASD doesn't move the player while the inventory is open.
- Mouse clicks on egui panels don't pass through to the world. Clicks outside any panel still interact with the game.
- Esc closes the topmost window, or opens the settings menu if nothing else is open.
- The simulation **continues running** while non-settings windows are open (inventory, build selector, tech tree). Only the settings menu pauses the tick.
