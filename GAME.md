# Octofact — Eldritch Factory Game Design

## 1. Premise & Framing

You are a cartographer-engineer sent by a dying civilization to exploit the Surface — an infinite hyperbolic plane that shouldn't exist. It was discovered folded inside an artifact, and when unfolded, it kept going. The geometry is wrong. Parallel lines diverge. A square grid that should tile flat space instead buckles and proliferates, each neighborhood spawning more neighbors than it should. There is always more room.

Your civilization needs what grows here: materials whose crystalline structures require negative curvature to be stable. They cannot exist in flat space. They shatter into inert dust the moment you try to remove them from the Surface, so everything must be refined *in situ*, on the plane itself, and only the finished product — compressed into geometry-agnostic form — can be extracted.

You are alone on an alien mathematical surface with a fabricator, a rebase compass, and a mandate to build.

---

## 2. Resources & Processing Chains

### Raw Materials

Resources emerge from the Surface itself — mathematical primitives that crystallize in the negative curvature. Miners extract them from deposits within cells.

| Resource | Description | Spawn pattern |
|----------|-------------|---------------|
| **Null Sets** | The emptiness itself, structured. Crystallized absence that the Surface produces where the metric thins toward zero. | Common, evenly distributed |
| **Points** | Dimensionless position-objects. The atomic unit of geometry on the Surface. Dense clusters near tile centers. | Common, evenly distributed |
| **Preimages** | Ghosts of functions that haven't been applied yet. The Surface is full of latent mappings waiting to be realized. Shimmering, unstable until processed. | Uncommon, patchy distribution |
| **Wavelets** | Oscillatory fragments of the Surface's vibrational modes. The plane hums, and wavelets are the harvestable residue. | Uncommon, frontier-weighted |

### Processing Chains

The production chain is built on mathematical operations made physical. Machines don't smelt or assemble — they *compose*, *invert*, *embed*, *quotient*, and *transform*. The items they produce are mathematical objects: line segments, functions, cubes, standing waves. The factory is a proof engine.

Full item and recipe details are in [ITEMS.md](ITEMS.md).

#### Machines

Each machine type performs a single mathematical operation. Machines are themselves manufactured items — your factory builds its own means of production.

| Machine | Tier | Inputs | Outputs | Size | Operation |
|---------|------|--------|---------|------|-----------|
| Composer | T1 | 1 | 1 | 3x3 | Combines items into composite structures. The workhorse. |
| Inverter | T1 | 1 | 1 | 1x1 | Reverses a mapping. Turns preimages into functions, cubes into Necker cubes. |
| Embedder | T2 | 2 | 1 | 2x2 | Maps one object into the structure of another. |
| Quotient | T2 | 2 | 2 | 2x4 | Divides one structure by another, producing both the quotient and remainder. |
| Transformer | T2 | 3 | 3 | 6x3 | Applies a transformation across multiple inputs simultaneously. |

#### Tier 1 — Composition & Inversion

Built from raw resources using Composers and Inverters. This is where geometry bootstraps into algebra.

| Recipe | Machine | Inputs | Output |
|--------|---------|--------|--------|
| Line Segment | Composer | 2x Point | 1x Line Segment |
| Exact Sequence | Composer | 3x Preimage | 1x Exact Sequence |
| Identity | Composer | 1x Null Set | 1x Identity |
| Square | Composer | 4x Line Segment | 1x Square |
| Cube | Composer | 6x Square | 1x Cube |
| Standing Wave | Composer | 2x Wavelet | 1x Standing Wave |
| Function | Inverter | 1x Preimage | 1x Function |
| Necker Cube | Inverter | 1x Cube | 1x Necker Cube |
| Image | Inverter | 1x Preimage | 1x Image |

**Self-bootstrapping items** — the factory builds itself:

| Recipe | Machine | Inputs | Output | Notes |
|--------|---------|--------|--------|-------|
| Belt | Composer | 1x Line Segment | 1x Belt | You manufacture your own logistics. |
| Axiomatic Science | Composer | 1x Cube | 1x Axiomatic Science | Research feedstock. |
| Composer | Composer | 2x Function | 1x Composer | The machine that builds itself. |
| Inverter | Inverter | 1x Composer | 1x Inverter | Bootstrap from a Composer. |
| Knowledge Sheaf | Composer | 12x Axiomatic Science | 1x Knowledge Sheaf | Science consumer (5x5). |

**Power chain:**

| Recipe | Machine | Inputs | Output | Notes |
|--------|---------|--------|--------|-------|
| Quadrupole | Composer | 4x Identity | 1x Quadrupole | Electrical pole. |
| Dynamo | Composer | 2x Quadrupole | 1x Dynamo | Power generator. |

#### Tier 2 — Embedding

Embedders combine T1 products into higher-dimensional structures. This is where things get strange.

| Recipe | Machine | Inputs | Output |
|--------|---------|--------|--------|
| Root of Unity | Embedder | Preimage + Unity | 1x Root of Unity |
| Kernel | Embedder | Identity + Preimage | 1x Kernel |
| Quantum | Embedder | Standing Wave + Cube | 1x Quantum |

#### Later Tiers (TBD)

Quotients and Transformers enable higher-tier production chains. The endgame items — Klein Bottles, Riemann Zeroes, Last Theorems, White Holes, Boltzmann Brains — build on the T1/T2 foundation but their exact recipes are unresolved. The extraction beacon remains the ultimate goal.

### Transport

Three physical transport systems, unlocked in sequence. No long-range teleportation, no abstracted logistics towers — everything moves through space, and the space is hyperbolic, so everything is harder than it looks.

**Belts** are the backbone. They follow grid edges, one hop per tick. Items travel in Klein Bottles. Belts connect directly into buildings — no inserters, no loader arms. A belt entering a Shaper's footprint feeds it; a belt exiting carries product. Buildings have designated input/output faces (marked on placement). This is the Satisfactory model: the belt *is* the interface.

**Tunnel belts** are short-range quantum tunnels. Place an entrance and an exit up to 16 grid squares apart (in a straight line), and items phase through the intervening space underground. Same throughput as a surface belt, same tick rate — the item enters the tunnel entrance and exits 16 ticks later at the other end. Visually, the entrance and exit are matching portal frames with a faint shimmer between them. The intervening grid squares are free for other buildings or belt crossings.

Tunnel belts solve the crossing problem: two belt lines that need to pass through each other without a splitter. They're the Factorio underground belt, flavored as the Surface's geometry being locally exploitable — at short range, you can punch through the metric and skip a few grid squares. The 16-square limit is a hard physical constraint; the tunnel destabilizes beyond that distance. Higher-tier tunnel belts (unlocked via research) extend the range — 32, 64 — but never to the point of replacing surface belts for long runs.

Belt routing is the core logistical challenge: hyperbolic space means belt networks that look local on the Poincare disk are actually covering enormous metric distances, and the geodesic between two points curves through the disk. Long belt runs visibly bend. Designing a belt network that doesn't knot itself is a genuine puzzle, and the higher the n in {4, n}, the worse it gets.

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

**Cell corners are dead zones.** At a vertex of the hyperbolic tiling, n cells meet (not 4, as in flat space). The geometry at these corners is ambiguous — which cell does a grid square at the corner belong to? Rather than solve this, building is banned in a small exclusion zone around each corner (a few grid squares). Nothing can be placed there: no belts, no pipes, no structures. The corners are where the hyperbolic curvature concentrates, and the game makes that visible by leaving them empty. Transport crosses cell boundaries only along edges, not through corners.

### Building

Structures are placed on the 128x128 internal grid of a cell. Sizes below are in grid squares.

| Structure | Size | Function |
|-----------|------|----------|
| Miner | 3x3 | Extracts raw resource. Must be placed on a deposit within the cell. |
| Belt | 1x1 | Moves solid items one grid square per tick. Connects directly into buildings. Manufactured item (Composer + Line Segment). |
| Tunnel Belt | 1x1 | Quantum tunnel entrance/exit pair. Items phase underground for up to 16 grid squares. |
| Splitter | 2x2 | 4-port junction. Configurable as 1-in/3-out, 3-in/1-out, or 2-in/2-out. See below. |
| Pipe | 1x1 | Carries fluids. Can layer under belts on the same grid square. |
| Pump | 2x2 | Drives fluid pressure through pipes. Required every ~40 grid squares. |
| Rail | 1x1 | Train track segment. Carries bulk throughput over long distances. |
| Train Station | 8x4 | Load/unload point for train routes |
| Composer | 3x3 | T1 machine. 1 input, 1 output. Manufactured item (Composer + 2x Function). |
| Inverter | 1x1 | T1 machine. 1 input, 1 output. Manufactured item (Inverter + Composer). |
| Embedder | 2x2 | T2 machine. 2 inputs, 1 output. |
| Quotient | 2x4 | T2 machine. 2 inputs, 2 outputs. |
| Transformer | 6x3 | T2 machine. 3 inputs, 3 outputs. |
| Knowledge Sheaf | 5x5 | T1 science consumer. Feeds the tech tree. |
| Quadrupole | 3x3 | Electrical pole. Transmits power across the grid. |
| Dynamo | 5x5 | Power generator. Composed from 2x Quadrupole. |
| Extraction Beacon | 21x21 | Endgame structure. Folds goods into flat-space-compatible form. |

**Belt I/O.** No inserters. Belts plug directly into building faces. Each building has designated input and output faces shown during placement preview. A Miner has output faces only. A Composer has one input face and one output face. A Quotient has two input faces and two output faces. The player rotates the building to align its faces with the belt layout. This keeps logistics visually legible — you can trace the flow by following the belts, no invisible arm mechanics.

**Splitters** are 4-port junctions (DSP-style). Each port is configurable as input or output. The three standard modes:

- **1-in / 3-out (splitting):** One input belt distributes evenly across three output belts. Round-robin distribution, one item per output per cycle.
- **3-in / 1-out (merging):** Three input belts merge onto one output. Priority is configurable (round-robin default, or set one input as primary).
- **2-in / 2-out (balancing):** Two inputs, two outputs. Items distribute evenly across both outputs regardless of which input they arrived on. The classic bus balancer.

Splitters can also filter by item type on each port — set a port to only accept Points, and everything else backs up or routes to the other ports. On hyperbolic grids where cell vertices have n > 4 meeting points, splitters placed near boundaries are especially powerful because the extra adjacency gives belt networks more routing options in tight spaces.

### Expansion

You start in the origin cell with 128x128 grid squares, a handful of starter Composers, and nothing else. Early game happens entirely within this one cell — mine Null Sets and Points, compose your first Line Segments, build Belts, bootstrap more Composers. It feels like a normal factory game.

Then you run out of room, or you need a resource that doesn't spawn in the origin cell, and you step across a cell boundary for the first time. The neighboring cell is another 128x128 grid, but getting materials back to your origin hub now requires belts or rail that cross the boundary. And the cell after that is another boundary crossing. And there are more neighbors than you expected, because n > 4 cells meet at each vertex.

The visible tiling grows outward via BFS from the player's current neighborhood. Explored cells persist — buildings stay where you put them — but the rendered region is bounded. The rebase compass is your lifeline: it tells you the canonical direction back to origin (or to any beacon you've placed), because in hyperbolic space, getting lost is geometrically easy. Every direction looks the same, and there are exponentially many ways to go wrong.

### Power

Power comes from Dynamos (composed from 2x Quadrupole, which are themselves composed from 4x Identity). Quadrupoles transmit power along adjacency chains across the grid. The hyperbolic branching means a small number of poles cover an enormous number of grid squares — but a single broken link in an exponentially-branching power grid is hard to find and diagnose.

The power chain bootstraps from raw Null Sets: Null Set → Identity → Quadrupole → Dynamo. This means power infrastructure competes for Composer time and Null Set supply with everything else in the early game.

### Research

Research is driven by Knowledge Sheaves (5x5, science consumer). Each Knowledge Sheaf consumes Axiomatic Science (composed from Cubes) to advance the tech tree. The production chain is: Point → Line Segment → Square → Cube → Axiomatic Science → Knowledge Sheaf.

This means research competes for Composer time and Point supply with everything else. A dedicated science production line — miners feeding Points into a Composer chain that terminates at a Knowledge Sheaf — is an early-game priority.

The tech tree unlocks T2 machines (Embedders, Quotients, Transformers), higher-tier recipes, and infrastructure upgrades. Some branches are only reachable if you've built infrastructure at sufficient canonical depth — the Surface rewards those who push outward.

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

- **Tiles** shift from the current rainbow HSV cycle to a terrain palette — dark basalt, veined crystal, shimmering metric distortion. Deposits of Null Sets are dark voids; Point clusters are bright pinpricks; Preimage deposits shimmer with unrealized potential; Wavelet patches oscillate faintly.
- **Structures** are mathematical operations made visible. A Composer is interlocking rotating rings. An Inverter is a mirrored prism. A Transformer is three parallel channels with rotating matrix motifs. Quadrupoles radiate visible field lines. Dynamos hum with rotating cores.
- **Belts** carry items — tiny mathematical objects in Klein Bottles that catch the light wrong. Their curvature through the disk is part of the visual identity — the way they bend reveals the geometry.
- **The disk boundary** is not just a fade-out but a presence: an encroaching dark, suggesting the Surface extends further than you can perceive. Things move at the edge of visibility.
- **Knowledge Sheaves** glow when actively consuming Axiomatic Science, pages fanning through proofs.

### Atmosphere

Cosmic industrial solitude. You are running a factory on a surface that violates the geometry of your home universe. The materials are beautiful and wrong. The deeper you go, the stranger the Surface becomes. Tiles at extreme canonical depth have visual glitches. The palette shifts. Your compass takes longer to resolve. There is nothing hostile here — no enemies, no threats, no combat. Just the geometry, the work, and the growing suspicion that the Surface is larger than you can comprehend.

The horror is the geometry itself: infinite, proliferating, indifferent. You are a small operation on an incomprehensibly large surface, and every direction you expand reveals that there is always more.

### Asset Pipeline

No external modeling tools. The geometric/abstract aesthetic is a strength — everything can be expressed as math.

**Phase 1: Procedural geometry.** All structures are Rust functions that emit `Vec<Vertex>` + `Vec<u16>`, extending the existing `build_polygon_mesh` pattern. Composers are interlocking rings, Inverters are mirrored prisms, Transformers are tri-channel machines. Belt segments are instanced trough-shaped ribbons. Every shape is code, living next to the math it represents.

**Phase 2: SDF raymarching for showcase objects.** A second render pass for late-game objects that *should* look impossible. Small on screen, bounded in count.

Implementation details — belt rendering, instancing strategy, pipes, rail, structure geometry sketches — are in [GRAPHICS.md](GRAPHICS.md).

### Sound (Future)

- Ambient: low harmonic drones that shift pitch based on local curvature density
- Structures: mechanical rhythms, smelter hum, belt clatter
- Deep-plane: the drones become dissonant, occasional tonal intrusions from outside the audible range (felt more than heard)

---

## 6. User Configuration

Build the settings layer early, before the codebase accumulates ad-hoc input handling and hardcoded defaults. All screen-space UI is rendered via **egui** (`egui-wgpu` + `egui-winit`). See [GRAPHICS.md](GRAPHICS.md) for integration details.

### In-Game Windows

| Window | Opens via | Contents |
|--------|-----------|----------|
| Build Selector | B or toolbar | Grid of unlocked structures, grouped by category. Click or hotkey to select, then place in world. |
| Inventory | I or Tab | Current resource counts, item totals, production/consumption rates. |
| Tech Tree | T | Branching research graph. Locked nodes greyed out. Knowledge Sheaves consume Axiomatic Science to advance. |
| Settings | Esc | Key bindings, graphics, gameplay, audio. Pauses the simulation. |
| Milestone Log | M | Completed and upcoming milestones. |
| Cell Info | Click cell border | Canonical address, resource deposits, structure count for the selected cell. |

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

**Milestone-based sandbox.** No hard win condition, no credits screen. Instead, a sequence of escalating milestones that give structure without an ending: first Line Segment, first Function, first self-built Composer, first Dynamo, first Embedder recipe, first Extraction Beacon, reach canonical depth 20, reach depth 50, etc. Milestones unlock cosmetic rewards or new ambient details (the Surface acknowledges your presence the deeper you go). You can always go deeper. You can never finish.

**Single-player.** Multiplayer is out of scope. The isolation is thematically load-bearing. (The Surface is theoretically large enough to share — two players could build toward each other across exponential space — but this is a someday-maybe, not a design target.)

**Persistence.** Save only actually discovered cells, not everything within a radius. There might be 3 billion cells within 20 hops of origin, but the player will only visit a thin tree of paths through that space. Undiscovered cells don't exist in the save file — they're generated on first visit from a deterministic seed keyed to their canonical address. Discovered cells persist their 128x128 grid state (structures, belt contents, resource depletion). This keeps save files proportional to the player's actual footprint, not the exponential volume of the space they've nominally "reached."
