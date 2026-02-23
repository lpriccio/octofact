# Octofact — Eldritch Factory Game Design

## 1. Premise & Framing

You are a cartographer-engineer sent by a dying civilization to exploit the Surface — an infinite hyperbolic plane that shouldn't exist. It was discovered folded inside an artifact, and when unfolded, it kept going. The geometry is wrong. Parallel lines diverge. A square grid that should tile flat space instead buckles and proliferates, each neighborhood spawning more neighbors than it should. There is always more room.

Your civilization needs what grows here: materials whose crystalline structures require negative curvature to be stable. They cannot exist in flat space. They shatter into inert dust the moment you try to remove them from the Surface, so everything must be refined *in situ*, on the plane itself, and only the finished product — compressed into geometry-agnostic form — can be extracted.

You are alone on an alien mathematical surface with a fabricator, a rebase compass, and a mandate to build.

---

## 2. Resources & Processing Chains

### Raw Materials

Resources emerge from the Surface itself, spawning at rates proportional to the local curvature density (more tiles nearby = richer yields in hyperbolic space, which means the further from your origin, the denser things get).

| Resource | Description | Spawn pattern |
|----------|-------------|---------------|
| **Geodesic Stress** | Raw curvature energy. The Surface is under tension everywhere — this is the stuff that makes parallel lines diverge. Crystallizes along tile edges. | Edge deposits, common |
| **Metric Foam** | The local fabric of space itself, skimmed where the metric is most dilated. Iridescent, slightly unsettling to look at. | Center deposits, common |
| **Boundary Flux** | Leaked energy from the Poincare disk boundary — the place where the model says "infinity." Condenses near the frontier of explored space. | Frontier deposits, uncommon |
| **Zero Ore** | Dense, black, and humming. Found only deep in the tiling. Named for what it contains: the raw substrate from which Riemann Zeroes can be extracted. | Deep deposits, rare |

### Processing Chains

The production chain climbs from raw geometry toward objects that shouldn't be able to exist — famous mathematical and physical paradoxes, impossible constructs, and theoretical mcguffins made real by the Surface's broken rules.

#### Tier 1 — Extraction

Miners sit on tiles and pull raw material. One miner per tile. Output rate depends on resource density.

#### Tier 2 — Refinement

Smelters and shapers combine raw materials into paradox components. Recipes use the geometry: a smelter must be adjacent to a specific number of input sources, and "adjacent" on hyperbolic grids means something different than on flat ones.

| Recipe | Inputs | Output | Notes |
|--------|--------|--------|-------|
| Klein Bottle | 4 Metric Foam | 1 Klein Bottle | The Surface's negative curvature allows non-orientable manifolds to close. Used as universal containers — they hold more than their volume because inside and outside aren't distinct. The basic logistics container for all higher-tier transport. |
| Magnetic Monopole | 3 Geodesic Stress + 1 Boundary Flux | 1 Monopole | Maxwell's equations lose a symmetry constraint on the Surface. Monopoles are the power source: each one generates a field that decays hyperbolically rather than inverse-square, making power transmission across exponential distances feasible. |
| Riemann Zero | 2 Zero Ore + 1 Klein Bottle | 1 Riemann Zero | Extracted, stabilized non-trivial zeroes of the zeta function. On the Surface, the critical strip has physical extent — zeroes are *locations*, not just numbers. Each one is unique and acts as an information-dense seed for higher synthesis. Whether they all have real part 1/2 is... empirically likely, but the foundry has occasionally produced anomalous results. |
| Penrose Tile | 2 Metric Foam + 2 Geodesic Stress | 1 Penrose Tile | Aperiodic crystal fragments that the Surface generates spontaneously. Never repeat, never quite the same. Used as structural material — buildings made from Penrose Tiles are stable against geometric perturbation because they have no periodic weakness to exploit. |

#### Tier 3 — Synthesis

Assemblers combine Tier 2 components into impossible artifacts. These require larger footprints and precise spatial arrangements.

| Recipe | Inputs | Output | Notes |
|--------|--------|--------|-------|
| Last Theorem | 3 Riemann Zeroes + 2 Penrose Tiles | 1 Last Theorem | A crystallized proof-object. Fermat's marginal note, made physical: a structure that encodes the impossibility of a^n + b^n = c^n for n > 2 as a stable lattice constraint. Used as the core logic element in advanced assemblers — it enforces exact conservation laws on material flow by making violations *structurally impossible*. |
| White Hole | 2 Monopoles + 1 Last Theorem + 4 Klein Bottles | 1 White Hole | The time-reverse of a black hole. On the Surface, where the geometry permits closed timelike curves if you go deep enough, white holes are constructible: singularities that only emit. They are the endgame power source — a white hole outputs energy forever, but cannot be turned off or moved once placed. Choose the location wisely. |
| Boltzmann Brain | 5 Riemann Zeroes + 1 White Hole | 1 Boltzmann Brain | A spontaneous fluctuation into consciousness, pinned and stabilized by the information density of the zeroes and the inexhaustible output of the white hole. The ultimate compute substrate. A Boltzmann Brain doesn't run programs — it *is* every possible computation simultaneously, and you query it for the one you need. Used to automate entire factory sub-networks: feed it a logistics problem, and it has always already solved it. |

#### Tier 4 — Extraction Beacons

The endgame structure. An extraction beacon folds finished goods through a controlled geometric collapse — compressing the hyperbolic product into flat-space-compatible form for transmission off the Surface. Each beacon requires a White Hole (for power), a Boltzmann Brain (for the fold calculations), Last Theorems (for structural integrity), and a truly absurd quantity of Klein Bottles (for containment during the dimensional reduction).

The cruel joke: building one beacon is a triumph. But your civilization's needs are exponential. And the Surface, being hyperbolic, can always accommodate more.

### Transport

Three physical transport systems, unlocked in sequence. No teleportation, no abstracted logistics towers — everything moves through space, and the space is hyperbolic, so everything is harder than it looks.

**Belts** are the backbone. They follow tile edges, one hop per tick. Items travel in Klein Bottles. Belt routing is the core logistical challenge: hyperbolic space means belt networks that look local on the Poincare disk are actually covering enormous metric distances, and the geodesic between two points curves through the disk. Long belt runs visibly bend. Designing a belt network that doesn't knot itself is a genuine puzzle, and the higher the n in {4, n}, the worse it gets.

**Pipes** carry fluids — Metric Foam in its liquid state, coolant for Shapers, fuel for Monopole Towers. Pipes also follow tile edges but can be layered under belts on the same tile. Fluid flow is pressure-driven: pumps push, and the exponential branching of hyperbolic space means pressure drops faster than you'd expect over distance. Long pipe runs need relay pumps.

**Trains** are the mid-game answer to long-distance bulk transport. Rail lines are built tile-by-tile like belts but carry far more throughput. A train route from your origin hub to a deep-plane mining outpost is a serious infrastructure investment — dozens or hundreds of tiles of track, following a geodesic path that curves through the disk. Trains make the center-to-frontier supply problem tractable, but laying and maintaining rail across exponential space is itself a logistical challenge. Junction design on hyperbolic grids, where more tracks meet at each vertex than on a flat grid, is a rich puzzle.

**Late-game automation.** Boltzmann Nodes don't teleport items — they optimize the physical network in their vicinity, automatically rerouting belts and switching train junctions to solve throughput bottlenecks. They make your existing infrastructure smarter, not obsolete.

---

## 3. Core Mechanics

### The Two-Scale Grid

The world has two scales of structure:

**Macro: the hyperbolic tiling.** {4, n} squares connected by the curvature of the Surface. This is the scale of the Poincare disk, Mobius transforms, and canonical addresses. Each square is a *cell* — a distinct region of the Surface.

**Micro: the internal grid.** Each cell contains a 128x128 Euclidean grid. This is where you actually build. Structures, belts, pipes, and rail are placed on grid squares within a cell. Inside a single cell, the geometry is flat — a normal factory grid. The hyperbolic curvature only matters at the cell boundaries, where neighboring cells meet at angles that don't add up to 360 degrees.

128x128 = 16,384 build slots per cell. That's enough to hold a starter factory in the origin cell, or a major processing hub, or a dense rail junction. It is *not* enough to hold an entire late-game production chain. You will outgrow your first cell. You will outgrow your first several cells. The Surface makes sure of that.

### Cell Boundaries

When a belt, pipe, or rail reaches the edge of a cell, it can connect to the corresponding edge of the neighboring cell. Each cell has 4 edges (it's a square), and each edge is 128 grid units long. The connection points at cell boundaries are where the Euclidean interior meets the hyperbolic exterior — this is where logistics gets interesting.

At a vertex of the hyperbolic tiling, n cells meet (not 4, as in flat space). A belt exiting the corner of a cell has more possible destination cells than on a flat grid. Junction design at cell vertices — where n cells' corners converge — is a unique puzzle that doesn't exist in Euclidean factory games.

### Building

Structures are placed on the 128x128 internal grid of a cell. Sizes below are in grid squares.

| Structure | Size | Function |
|-----------|------|----------|
| Miner | 3x3 | Extracts raw resource. Must be placed on a deposit within the cell. |
| Belt | 1x1 | Moves solid items one grid square per tick. Items travel in Klein Bottles. |
| Splitter | 1x1 | Routes belt items to 2-4 outputs based on rules |
| Pipe | 1x1 | Carries fluids. Can layer under belts on the same grid square. |
| Pump | 2x2 | Drives fluid pressure through pipes. Required every ~40 grid squares. |
| Rail | 1x1 | Train track segment. Carries bulk throughput over long distances. |
| Train Station | 8x4 | Load/unload point for train routes |
| Shaper | 5x5 | Tier 2 processing — smelts raw materials into paradox components |
| Assembler | 9x9 | Tier 3 synthesis — combines components into impossible artifacts |
| Monopole Tower | 3x3 | Powered by a Magnetic Monopole. Transmits power across cells. |
| White Hole Anchor | 15x15 | Late-game power source. Permanent, unmovable, infinite output. |
| Boltzmann Node | 7x7 | Late-game optimizer. Auto-routes belts and switches train junctions in its vicinity. |
| Extraction Beacon | 21x21 | Endgame structure. Folds goods into flat-space-compatible form. |

### Expansion

You start in the origin cell with 128x128 grid squares and nothing else. Early game happens entirely within this one cell — mine local deposits, build your first Shapers, lay short belt runs. It feels like a normal factory game.

Then you run out of room, or you need a resource that doesn't spawn in the origin cell, and you step across a cell boundary for the first time. The neighboring cell is another 128x128 grid, but getting materials back to your origin hub now requires belts or rail that cross the boundary. And the cell after that is another boundary crossing. And there are more neighbors than you expected, because n > 4 cells meet at each vertex.

The visible tiling grows outward via BFS from the player's current neighborhood. Explored cells persist — buildings stay where you put them — but the rendered region is bounded. The rebase compass is your lifeline: it tells you the canonical direction back to origin (or to any beacon you've placed), because in hyperbolic space, getting lost is geometrically easy. Every direction looks the same, and there are exponentially many ways to go wrong.

### Power

Power comes from Magnetic Monopoles. In flat space, a point charge's field decays as 1/r^2. A monopole on the hyperbolic plane decays differently — the exponential growth of area with distance means the field thins faster, but a monopole's inherent symmetry-breaking means it can couple to the geometry itself. In practice: Monopole Towers transmit power along adjacency chains, and the hyperbolic branching means a small number of towers cover an enormous number of tiles. But a single broken link in an exponentially-branching grid is hard to find and diagnose.

Late-game, White Hole Anchors replace monopole networks for major installations. A white hole outputs energy forever and cannot be exhausted — but it also cannot be turned off, moved, or disassembled. Placing one is a permanent commitment to that region of the Surface.

### Research

The tech tree is organized around increasingly impossible constructs. Early research unlocks Klein Bottles and Monopoles. Mid-game research requires Riemann Zeroes as input — each zero consumed advances a branch of the tree, and since each zero is unique, the order you feed them in matters. Late-game research demands Last Theorems, which act as proof-of-concept: you must demonstrate that your factory can produce these impossibilities before the tree grants access to White Holes and Boltzmann Brains.

Some branches are only reachable if you've built infrastructure at sufficient canonical depth — the Surface rewards those who push outward. The tree branches exponentially, like everything else here.

---

## 4. How Hyperbolic Geometry Shapes Gameplay

This is not a flat factory game with a weird map projection. The geometry is the game.

### Exponential growth of space

In Euclidean space, the number of tiles within distance r grows as r^2. On the hyperbolic plane, it grows exponentially. This means:

- **You will never run out of room.** Expansion is always possible, in every direction, forever. There is no "map edge."
- **Logistics scale differently.** A factory that would be compact on a flat grid sprawls across enormous metric distances. "Nearby" has a different meaning when your neighborhood contains exponentially many tiles.
- **Scouting is crucial.** You can't survey the area around your base by glancing at a minimap. The region within 10 hops of your position contains thousands of tiles.

### Transport is physical and it curves

On a flat grid, a belt from A to B is a straight line (Manhattan distance). On the hyperbolic plane, the geodesic between two tiles curves through the Poincare disk. Belt paths that look straight on your screen are actually following hyperbolic geodesics. Long belts visibly bend. Pipes lose pressure faster than expected. Train routes arc through the disk in ways that defy Euclidean intuition.

There is no teleportation. No logistics drones, no abstracted point-to-point shipping. Every item physically traverses the Surface, tile by tile, through belts, pipes, and rail you built with your own hands. This is the design commitment that makes hyperbolic geometry matter: if you could teleport goods, the curvature would be cosmetic. Because everything moves through space, the shape of space is the shape of your logistics problem.

### No global coordinates

There is no absolute grid. Every position is defined relative to a path from the origin (the canonical address system). The Mobius transform that centers your view means the world is always being re-expressed relative to where you stand.

In practice: you can't just write down "build at (47, 23)." You navigate to a place, and the place is defined by how you got there. The rebase compass and landmark beacons become essential navigation tools.

### Adjacency is richer

Each cell has 4 edges and 4 corners, just like a flat square. But at each corner vertex, n cells meet instead of 4. This means:

- **Cell-boundary junctions are more complex.** Where belt or rail lines exit a cell corner, there are n-1 possible destination cells instead of 3. Junction design at these vertices is a unique puzzle.
- **More neighbors means more routes.** Each cell borders 4 edge-neighbors and shares vertices with additional cells beyond those. The network topology is denser than flat space.
- **But path-counting is harder.** The branching factor of the cell graph is higher, so finding optimal routes between distant cells is computationally and cognitively more difficult. Within a cell, the 128x128 grid is Euclidean and familiar. The hyperbolic complexity lives at the boundaries.

### The center is precious

The origin cell — 128x128 grid squares, where you first arrived — is the one distinguished location on the Surface. Everything else is defined relative to it. And because all transport is physical, the origin cell naturally becomes the hub.

In hyperbolic space, the shortest path between two frontier cells almost always passes through (or near) the center. Two mining outposts at canonical depth 15 in different directions are astronomically far from each other, but both are only 15 cell hops from origin. This makes the origin cell the natural location for your main processing complex, your train junction, your pipeline manifold. Raw materials flow inward from frontier mines along spoke routes; finished goods flow outward to extraction beacons.

The tension: 128x128 is a lot of space, but it's finite. Every spoke route from the frontier enters the origin cell through one of its 4 edges, and each edge is only 128 grid units wide. As your factory scales, the origin cell becomes a dense tangle of belts, pipes, and rail — a logistics knot that you're constantly redesigning to fit one more trunk line through. You can offload processing to neighboring cells, but those cells are also transit corridors for deeper spokes. The center has the best logistics and the worst congestion, and you can never quite solve it, only push the bottleneck around.

---

## 5. Aesthetic Direction

### Visual

The current renderer already establishes the mood: a gentle bowl of colored tiles curving away in every direction, fading at the disk boundary, lit from above. The factory game extends this:

- **Tiles** shift from the current rainbow HSV cycle to a terrain palette — dark basalt, veined crystal, shimmering metric distortion. Tiles rich in Zero Ore are black and faintly vibrating. Boundary Flux deposits glow at the edges.
- **Structures** are built from Penrose Tiles and impossible geometry. A Shaper is a slowly rotating non-orientable surface. A Monopole Tower is a single point source with visible field lines radiating outward along the hyperbolic grid. A White Hole Anchor is a permanent scar of light — blinding at the center, dimming along geodesics.
- **Belts** carry Klein Bottles — tiny glass-like vessels that catch the light wrong. Their curvature through the disk is part of the visual identity — the way they bend reveals the geometry.
- **The disk boundary** is not just a fade-out but a presence: an encroaching dark, suggesting the Surface extends further than you can perceive. Things move at the edge of visibility.
- **Boltzmann Nodes** flicker. They display, for single frames, solutions to problems you haven't posed yet.

### Atmosphere

Cosmic industrial solitude. You are running a factory on a surface that violates the geometry of your home universe. The materials are beautiful and wrong. The deeper you go, the stranger the Surface becomes. Tiles at extreme canonical depth have visual glitches. The palette shifts. Your compass takes longer to resolve. There is nothing hostile here — no enemies, no threats, no combat. Just the geometry, the work, and the growing suspicion that the Surface is larger than you can comprehend.

The horror is the geometry itself: infinite, proliferating, indifferent. You are a small operation on an incomprehensibly large surface, and every direction you expand reveals that there is always more.

### Asset Pipeline

No external modeling tools. The geometric/abstract aesthetic is a strength — everything can be expressed as math.

**Phase 1: Procedural geometry.** All structures are Rust functions that emit `Vec<Vertex>` + `Vec<u16>`, extending the existing `build_polygon_mesh` pattern. A Shaper is a truncated icosahedron. A Monopole Tower is a tapered cylinder with radial fins. Belt segments are extruded ribbons along tile edges. Train stations are beveled rectangular prisms. Every shape is code, living next to the math it represents. Iteration cycle is compile-run-look, but for parametric geometry that's fast enough.

**Phase 2: SDF raymarching for showcase objects.** Add a second render pass that raymarches signed distance functions in WGSL for the objects that *should* look impossible. Klein Bottles on belt lines (parametric SDF, non-orientable surface rendered correctly). Monopole field visualizations (radial field lines decaying along the hyperbolic grid). White Hole singularities (glowing emission, no surface, just light). Boltzmann Node flicker effects (stochastic SDF perturbation). These are small on screen and bounded in count, so the per-pixel cost is manageable. SDF lighting must match the rasterized tile lighting to stay cohesive.

The bulk of the world — tiles, belts, pipes, rail, building footprints, terrain — stays rasterized procedural geometry forever. SDFs are reserved for the handful of objects where the math *is* the visual, and meshing them would lose the point.

### Sound (Future)

- Ambient: low harmonic drones that shift pitch based on local curvature density
- Structures: mechanical rhythms, smelter hum, belt clatter
- Deep-plane: the drones become dissonant, occasional tonal intrusions from outside the audible range (felt more than heard)

---

## 6. User Configuration

Build the settings layer early, before the codebase accumulates ad-hoc input handling and hardcoded defaults.

### Settings Menu

Esc opens a pause/settings overlay. The game world freezes (or dims) behind it. Menu is navigable by keyboard. Sections:

- **Key Bindings** (priority — see below)
- **Graphics** — render distance (BFS depth), resolution scale, frame rate cap
- **Gameplay** — tiling parameter {4, n}, tick rate
- **Audio** — volume sliders (when audio exists)

### Key Bindings

First-class rebindable controls. Every game action maps to a named action, and the player binds keys to actions rather than actions being hardcoded to keys. This replaces the current hardcoded WASD/QE/L handling in `app.rs`.

**Default bindings:**

| Action | Default | Notes |
|--------|---------|-------|
| Move forward | W | Hyperbolic translation |
| Move backward | S | |
| Strafe left | A | |
| Strafe right | D | |
| Camera up | Q | |
| Camera down | E | |
| Toggle labels | L | Debug overlay |
| Open settings | Esc | |
| Place structure | Left click | |
| Remove structure | Right click | |
| Rotate structure | R | |
| Quick-save | F5 | |

**Implementation notes:**

- Store bindings in a user config file (TOML or JSON) outside the save game — bindings are per-player, not per-save.
- Support multi-key detection for rebinding (press-to-set UI).
- Actions are an enum. Input layer translates raw key events into action events. Nothing downstream knows about physical keys.
- Conflict detection: warn if two actions share a binding, but allow it (some players want overlapping binds).

### Config Persistence

Settings persist to a config file in a platform-appropriate location (`~/.config/octofact/` on macOS/Linux). The game reads defaults on first launch and creates the file. In-game changes write back immediately.

Config is separate from save data. A fresh install with an existing config file should pick up the player's bindings and preferences without a save game present.

---

## 7. Design Decisions

**Tiling parameters / difficulty.** The value of n in {4, n} is configurable and acts as the difficulty axis. {4, 5} is easy mode: five squares meet at each vertex, curvature is gentle, space grows slowly, logistics stay manageable. {4, 8} is hard mode: eight squares per vertex, extreme curvature, space explodes outward, belt routes become fiendishly complex, and the exponential resource scaling means you're always behind. Mid-range values ({4, 6}, {4, 7}) offer graduated challenge.

**Simulation: fixed tick.** Discrete simulation steps, Factorio-style. Belts move items one hop per tick. Pipes push fluid one segment per tick. Trains advance along rail per tick. No continuous interpolation, no floating-point drift on curved paths. Deterministic and reproducible. Visual smoothing between ticks is a rendering concern, not a simulation concern.

**Flat only.** No stacking, no bridges, no vertical gameplay axis. The hyperbolic grid provides more than enough complexity. Height remains cosmetic (the existing click-to-raise is debug/aesthetic only). This simplifies building placement, collision, rendering, and saves an entire dimension of logistics headaches.

**Milestone-based sandbox.** No hard win condition, no credits screen. Instead, a sequence of escalating milestones that give structure without an ending: first Klein Bottle, first Monopole, first Riemann Zero, first Extraction Beacon, reach canonical depth 20, reach depth 50, build 10 beacons, etc. Milestones unlock cosmetic rewards or new ambient details (the Surface acknowledges your presence the deeper you go). You can always go deeper. You can never finish.

**Single-player.** Multiplayer is out of scope. The isolation is thematically load-bearing. (The Surface is theoretically large enough to share — two players could build toward each other across exponential space — but this is a someday-maybe, not a design target.)

**Persistence.** Save only actually discovered cells, not everything within a radius. There might be 3 billion cells within 20 hops of origin, but the player will only visit a thin tree of paths through that space. Undiscovered cells don't exist in the save file — they're generated on first visit from a deterministic seed keyed to their canonical address. Discovered cells persist their 128x128 grid state (structures, belt contents, resource depletion). This keeps save files proportional to the player's actual footprint, not the exponential volume of the space they've nominally "reached."
