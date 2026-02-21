# Octofact — Project Status

## Current Phase: Complete (all 7 phases)

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Scaffold + blank window | Done |
| 2 | Hyperbolic math (Complex, Mobius) | Done |
| 3 | Tiling state (BFS + embedding) | Done |
| 4 | Basic rendering pipeline | Done |
| 5 | Cell labels (glyphon 0.10) | Done |
| 6 | Camera & WASD movement | Done |
| 7 | Visual polish | Done |

## Test Results

19 unit tests, all passing:
- Complex arithmetic (4 tests)
- Mobius transforms (4 tests)
- Canonical octagon / neighbors (2 tests)
- Tiling BFS (5 tests)
- Hyperboloid embedding (3 tests)
- Address formatting (1 test)

## Controls

| Key | Action |
|-----|--------|
| W/A/S/D | Move on hyperbolic plane |
| Q | Raise camera |
| E | Lower camera |
| L | Toggle cell labels |
| Escape | Quit |

## Architecture

```
src/
  main.rs                     # entry point
  app.rs                      # GpuState, LabelState, App (ApplicationHandler)
  hyperbolic/
    poincare.rs               # Complex, Mobius, canonical_octagon, neighbor_transforms
    tiling.rs                 # Tile, TilingState (BFS + spatial dedup), format_address
    embedding.rs              # disk_to_hyperboloid (Y-up)
  render/
    mesh.rs                   # Vertex (32 bytes), concentric ring subdivision
    pipeline.rs               # Uniforms, RenderState (dynamic uniform buffer)
    camera.rs                 # (stub — camera logic integrated into app.rs)
    shader.wgsl               # Mobius + hyperboloid vertex; HSV palette + lighting fragment
```

## Tech Stack

- Rust, wgpu 28, winit 0.30, glyphon 0.10 (cosmic-text 0.15)
- Apple Metal backend
- {8,3} octagon disk radius: ~0.3647
