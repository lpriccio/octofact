
raw resources
-------------
null sets
points
preimages
wavelets


machines
-------------
composers - T1, 1 input, 1 output,  3x3 footprint
inverters - T1, 1 input, 1 output,  1x1 footprint
embedders - T2, 2 inputs, 1 output, 2x2 footprint
quotients - T2, 2 inputs, 2 outputs, 2x4 footprint
transfomers - T2, 3 inputs, 3 outputs, 6x3 footprint

knolwedge sheaf - T1, science consumer, 5x5 footprint

first tier manuf items (also creatable by player)
--------------
line segment <- compose(2x point)
exact sequences <- compose(3x preimage)
identity <- compose(1x null set)
square <- compose(4x line segment)
cube <- compose(6x square)
standing wave <- compose(2x wavelet)
function <- invert(preimage)
necker cube <- invert(cube)
image <- invert(preimage)

belt <- compose(line segment)
axiomatic science <- compose(cube)
composer <- compose(2x function)
inverter <- invert(composer)
knolwedge sheaf <- compose(12x axiomatic science)

quadropole <- compose(4x identity) -- 'electrical pole'
dynamo <- compose(2x quadropole) -- 'power generator 


second tier mauf iterms
-----------------------
root of unity <- embed(preimage, unity)
kernel <- embed(identity, preimage)
quantum <- embed(standing wave, cube)

