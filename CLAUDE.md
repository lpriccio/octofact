# Octofact

{4,n} hyperbolic plane factory game — Rust + wgpu 28 on Apple Metal. Default {4,5}.

## Build & Run

All cargo/rustup commands need the PATH prefix:

```sh
PATH="$HOME/.cargo/bin:$PATH" cargo build --release
PATH="$HOME/.cargo/bin:$PATH" cargo run --release
PATH="$HOME/.cargo/bin:$PATH" cargo clippy --release
```

Always use `--release` for interactive runs.

## wgpu 28 API Differences

These differ from older wgpu tutorials/examples:

| Struct / Method               | wgpu 28 (correct)                          | Common mistake                        |
|-------------------------------|-------------------------------------------|---------------------------------------|
| `DeviceDescriptor`            | has `experimental_features` field          | omitting it                           |
| `request_adapter`             | returns `Result`                           | treating it as `Option`               |
| `request_device`              | takes 1 arg (descriptor only)              | passing trace path as 2nd arg         |
| `RenderPassColorAttachment`   | has `depth_slice` field                    | omitting it                           |
| `RenderPassDescriptor`        | uses `multiview_mask`                      | using `multiview`                     |
| `PipelineLayoutDescriptor`    | uses `immediate_size`                      | using `push_constant_ranges`          |
| `RenderPipelineDescriptor`    | uses `multiview_mask`                      | using `multiview`                     |

## Architecture

```
src/
  app.rs                    # GpuState + App (ApplicationHandler), WASD, per-frame rebase
  hyperbolic/
    poincare.rs             # Complex, Mobius, canonical_polygon, neighbor_transforms
    tiling.rs               # Tile (canonical Vec<u8> address), TilingState (BFS + spatial dedup)
    embedding.rs            # disk_to_hyperboloid (Y-up)
  render/
    mesh.rs                 # Vertex (32 bytes), concentric ring subdivision (8 rings, 4 segs/side)
    pipeline.rs             # Uniforms (view_proj + mobius_a_b + disk_params), RenderPipeline
    camera.rs               # First-person Camera with view_mobius, rebase support
    shader.wgsl             # Mobius + hyperboloid in vertex, eldritch palette + lighting in fragment
```

## Tiling

Locked to **{4,n}** (square cells). The parameter n is configurable (default 5) and
controls curvature / difficulty. Cells are hyperbolic squares with a 64x64 internal grid.
Grid overlay uses the Klein model where geodesics are straight lines, so grid lines
are geodesics parallel to cell edges.

## Math Constants

For a generic {p,q} tiling:

- Circumradius (center to vertex): `cosh(chi) = cot(pi/p) * cot(pi/q)`
- Inradius (center to edge midpoint): `cosh(psi) = cos(pi/q) / sin(pi/p)`
- Half-edge length: `cosh(phi) = cos(pi/p) / sin(pi/q)`
- Poincare disk radius: `r_disk = tanh(chi/2)`
- Klein disk radius: `r_klein = 2*r_disk / (1 + r_disk^2)`
- Klein half-side ({4,n} square): `r_klein / sqrt(2)`

{4,5} square Poincare disk circumradius: **~0.3846**

```
cosh(chi) = cot(pi/4) * cot(pi/5) = 1.3764
r_disk = tanh(chi/2) = 0.3846
```

Center-to-center distance: `D = 2 * psi`, `cosh(D) = 2*cosh(psi)^2 - 1`

## Project Tracking

- `concept.md` — original vision and subgoals
- `PRD.md` — detailed product requirements (create if missing)
- `STATUS.md` — current project status (create if missing)
