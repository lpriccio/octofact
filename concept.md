# Goal: Factory game on the {4,n} square tiling of the hyperbolic plane

Originally {8,3} octagonal tiling; now locked to {4,n} square cells for grid-based gameplay.
Default {4,5}. See GAME.md for full design.

## Stack: Rust on Apple Metal

 you decide the rest

## Subgoal: canonize a combinatorial representation

Encoded as direction moved at each step from a distinguished origin point along a shortest path, of each cell in the tiling.  E.g. the point "00002" would be moving four times in the same direction and then once turning right by two sides of a polygon before moving.  Methods for reducing abritary paths to canonical form, so we can keep track of the geometry.  Feel free to use web search for this.

## Subgoal:  Poincare disk

Render the poincare disk representation of the tiling with the camera some distance above it.  

## Subgoal:  world model and display are seprate

Maintain a model of the poincare disk world, and **seperately** code up a render + camera + movement system to navigate it.

## Subgoal: graphical production values

Make the tiling colorful.  Color map determined by distance to origin modulo 16 or so.

## Subgoal: height

Let the user move the camera up or down from the disk with QE keys.

## Subgoal: interactivity

Let the user walk around with WASD.

## Subgoal: togglable canoncial form labels.

Give user a keystroke to turn on rendered text on each cell with its canonical representationw written on it.

## Subgoal: factory simulation

Fixed-timestep (60 UPS) simulation with gap-based belt transport, machine crafting, inserter transfers, and power networks. See GAME_PLAN.md for full architecture.

## Subgoal: instanced rendering

Replace per-tile draw calls with instanced rendering (~10 draw calls total). Per-instance Mobius transforms in vertex buffers.

## Subgoal: chunk streaming

Address-prefix chunks with ring loading around the player, LRU eviction, and freeze/thaw with fast-forward catch-up.

## Subgoal: save/load

Persist discovered cells, structures, belt contents, inventory, camera position. Undiscovered cells generated from deterministic seed.

## Keeping Track:

- `GAME_PLAN.md` — master architecture blueprint for factory game implementation
- `PRD.md` — detailed product requirements
- `GAME.md` — game design (resources, mechanics, aesthetics)
- `GRAPHICS.md` — rendering implementation details
- `ITEMS.md` — item and recipe definitions
- `STATUS.md` — current project status