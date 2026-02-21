# Octofact

{8,3} hyperbolic plane walking simulator — Rust + wgpu 28 on Apple Metal.

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
    poincare.rs             # Complex, Mobius, canonical_octagon, neighbor_transforms
    tiling.rs               # Tile (canonical Vec<u8> address), TilingState (BFS + spatial dedup)
    embedding.rs            # disk_to_hyperboloid (Y-up)
  render/
    mesh.rs                 # Vertex (32 bytes), concentric ring subdivision (4 rings, 3 segs/side)
    pipeline.rs             # Uniforms (view_proj + mobius_a_b + disk_params), RenderPipeline
    camera.rs               # First-person Camera with view_mobius, rebase support
    shader.wgsl             # Mobius + hyperboloid in vertex, eldritch palette + lighting in fragment
```

## Math Constants

For {p,q} = {8,3}:

- Circumradius (center to vertex): `cosh(chi) = cot(pi/p) * cot(pi/q) = cot(pi/8) * cot(pi/3)`
- Inradius (center to edge midpoint): `cosh(psi) = cos(pi/q) / sin(pi/p) = cos(pi/3) / sin(pi/8)`
- Half-edge length: `cosh(phi) = cos(pi/p) / sin(pi/q) = cos(pi/8) / sin(pi/3)`

{8,3} octagon Poincare disk circumradius: **~0.4056**

```
cosh(chi) = cot(pi/8) * cot(pi/3) = 1.3938
r_disk = tanh(chi/2) = 0.4056
```

Center-to-center distance: `D = 2 * psi`, `cosh(D) = 2*cosh(psi)^2 - 1`

## Project Tracking

- `concept.md` — original vision and subgoals
- `PRD.md` — detailed product requirements (create if missing)
- `STATUS.md` — current project status (create if missing)
