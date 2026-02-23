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

## Keeping Track:

PRD.md is a file with detailed product req document, STATUS.md documents current project status.  If these files do not exist, ask the user if they want to creat them.