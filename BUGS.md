# Known Bugs

when deleting athe first belt segment from a larger belt attached to the output of a machine (e.g., Source), the machine simply outputs on the next segment, rather than being unable to output.

when flying very far away form the origin (~25 cells), game grinds to a halt, apparently due to spawning of a lot more cells than should exist.  Floating point error maybe?  

clicking to select a building is sensitive only in the area near the base of the building, the rendered cuboid of the entire machine should be sensitive (no need for more complicated hitboxes than that)

