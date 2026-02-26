//! Machine port system — Satisfactory-style direct belt↔machine connections.
//!
//! Instead of separate inserter entities, machines have built-in input/output
//! ports. Belts connect directly to ports. Items transfer between belt endpoints
//! and machine input/output slots during the simulation tick.

use crate::game::items::MachineType;
use crate::game::world::Direction;

/// Whether a port accepts or produces items.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortKind {
    Input,
    Output,
}

/// A port definition in canonical orientation (machine facing North).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct PortDef {
    /// Which side of the machine cell this port is on (canonical, facing North).
    pub side: Direction,
    /// Input or output.
    pub kind: PortKind,
    /// Which machine slot this port maps to.
    /// For input ports: index into `input_slots`. For output: index into `output_slots`.
    pub slot: usize,
    /// Grid cell offset within the machine footprint where this port lives.
    /// (0,0) is the origin cell. For a 3×2 machine, valid offsets are (0..3, 0..2).
    pub cell_offset: (i32, i32),
}

/// Get the canonical port layout for a machine type (defined facing North).
///
/// Cell offsets are relative to the origin cell within the machine's footprint.
/// For a 3×2 machine (w=3, h=2), cells are:
/// ```text
///   (0,0) (1,0) (2,0)   ← North edge (y=0)
///   (0,1) (1,1) (2,1)   ← South edge (y=h-1)
/// ```
#[allow(dead_code)]
pub fn port_layout(machine_type: MachineType) -> &'static [PortDef] {
    use Direction::*;
    use PortKind::*;
    match machine_type {
        // Composer (2×2): input bottom-left, output top-left
        MachineType::Composer => &[
            PortDef { side: South, kind: Input, slot: 0, cell_offset: (0, 1) },
            PortDef { side: North, kind: Output, slot: 0, cell_offset: (0, 0) },
        ],
        // Inverter (3×2): input center-south, output center-north
        MachineType::Inverter => &[
            PortDef { side: South, kind: Input, slot: 0, cell_offset: (1, 1) },
            PortDef { side: North, kind: Output, slot: 0, cell_offset: (1, 0) },
        ],
        // Embedder (3×2): two inputs (south-center, west-top), output center-north
        MachineType::Embedder => &[
            PortDef { side: South, kind: Input, slot: 0, cell_offset: (1, 1) },
            PortDef { side: West, kind: Input, slot: 1, cell_offset: (0, 0) },
            PortDef { side: North, kind: Output, slot: 0, cell_offset: (1, 0) },
        ],
        // Quotient (3×2): input south-center, outputs north-center and east-top
        MachineType::Quotient => &[
            PortDef { side: South, kind: Input, slot: 0, cell_offset: (1, 1) },
            PortDef { side: North, kind: Output, slot: 0, cell_offset: (1, 0) },
            PortDef { side: East, kind: Output, slot: 1, cell_offset: (2, 0) },
        ],
        // Transformer (3×2): two inputs (south-center, west-top), output center-north
        MachineType::Transformer => &[
            PortDef { side: South, kind: Input, slot: 0, cell_offset: (1, 1) },
            PortDef { side: West, kind: Input, slot: 1, cell_offset: (0, 0) },
            PortDef { side: North, kind: Output, slot: 0, cell_offset: (1, 0) },
        ],
        // Source (1×1): single output on origin
        MachineType::Source => &[
            PortDef { side: North, kind: Output, slot: 0, cell_offset: (0, 0) },
        ],
    }
}

/// A port with its actual direction after rotation for the machine's facing.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct RotatedPort {
    /// Actual side of the machine cell (after rotation).
    pub side: Direction,
    /// Input or output.
    pub kind: PortKind,
    /// Machine slot index.
    pub slot: usize,
    /// Grid cell offset within the rotated footprint.
    pub cell_offset: (i32, i32),
}

/// Get the rotated port layout for a machine at the given facing direction.
///
/// Rotates both port side directions and cell offsets within the footprint.
#[allow(dead_code)]
pub fn rotated_ports(machine_type: MachineType, facing: Direction) -> Vec<RotatedPort> {
    let n = facing.rotations_from_north();
    let (w, h) = machine_type.footprint();
    port_layout(machine_type)
        .iter()
        .map(|def| RotatedPort {
            side: def.side.rotate_n_cw(n),
            kind: def.kind,
            slot: def.slot,
            cell_offset: facing.rotate_cell(def.cell_offset.0, def.cell_offset.1, w, h),
        })
        .collect()
}

/// Find which port (if any) is on the given side of a machine.
#[allow(dead_code)]
pub fn port_on_side(
    machine_type: MachineType,
    facing: Direction,
    side: Direction,
) -> Option<RotatedPort> {
    rotated_ports(machine_type, facing)
        .into_iter()
        .find(|p| p.side == side)
}

/// Find which port (if any) lives at a specific cell offset and faces a given side.
///
/// Used when a belt is adjacent to cell `cell_offset` of a machine: we need to know
/// if there's a port at that cell on the side facing the belt.
#[allow(dead_code)]
pub fn port_at_cell_on_side(
    machine_type: MachineType,
    facing: Direction,
    cell_offset: (i32, i32),
    side: Direction,
) -> Option<RotatedPort> {
    rotated_ports(machine_type, facing)
        .into_iter()
        .find(|p| p.side == side && p.cell_offset == cell_offset)
}

/// Determine whether a belt at an adjacent cell can connect to a machine port.
#[allow(dead_code)]
///
/// Given a machine at `(mx, my)` with a port on side `port_side`:
/// - The adjacent cell is at `(mx + port_side.dx, my + port_side.dy)`.
/// - For an **input port**: the belt must flow toward the machine
///   (belt direction = port_side.opposite()).
/// - For an **output port**: the belt must flow away from the machine
///   (belt direction = port_side).
///
/// Returns `true` if the belt direction is compatible with this port.
pub fn belt_compatible_with_port(port: &RotatedPort, belt_direction: Direction) -> bool {
    match port.kind {
        PortKind::Input => belt_direction == port.side.opposite(),
        PortKind::Output => belt_direction == port.side,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_has_one_input_one_output() {
        let ports = port_layout(MachineType::Composer);
        assert_eq!(ports.len(), 2);
        assert_eq!(
            ports.iter().filter(|p| p.kind == PortKind::Input).count(),
            1
        );
        assert_eq!(
            ports.iter().filter(|p| p.kind == PortKind::Output).count(),
            1
        );
    }

    #[test]
    fn embedder_has_two_inputs_one_output() {
        let ports = port_layout(MachineType::Embedder);
        assert_eq!(ports.len(), 3);
        assert_eq!(
            ports.iter().filter(|p| p.kind == PortKind::Input).count(),
            2
        );
        assert_eq!(
            ports.iter().filter(|p| p.kind == PortKind::Output).count(),
            1
        );
    }

    #[test]
    fn quotient_has_one_input_two_outputs() {
        let ports = port_layout(MachineType::Quotient);
        assert_eq!(ports.len(), 3);
        assert_eq!(
            ports.iter().filter(|p| p.kind == PortKind::Input).count(),
            1
        );
        assert_eq!(
            ports.iter().filter(|p| p.kind == PortKind::Output).count(),
            2
        );
    }

    #[test]
    fn rotation_north_is_identity() {
        let ports = rotated_ports(MachineType::Composer, Direction::North);
        assert_eq!(ports[0].side, Direction::South); // input
        assert_eq!(ports[1].side, Direction::North); // output
    }

    #[test]
    fn rotation_east_rotates_cw() {
        let ports = rotated_ports(MachineType::Composer, Direction::East);
        // South → West (input), North → East (output)
        assert_eq!(ports[0].side, Direction::West);
        assert_eq!(ports[0].kind, PortKind::Input);
        assert_eq!(ports[1].side, Direction::East);
        assert_eq!(ports[1].kind, PortKind::Output);
    }

    #[test]
    fn rotation_south_flips() {
        let ports = rotated_ports(MachineType::Composer, Direction::South);
        // South → North (input), North → South (output)
        assert_eq!(ports[0].side, Direction::North);
        assert_eq!(ports[0].kind, PortKind::Input);
        assert_eq!(ports[1].side, Direction::South);
        assert_eq!(ports[1].kind, PortKind::Output);
    }

    #[test]
    fn rotation_west() {
        let ports = rotated_ports(MachineType::Composer, Direction::West);
        // South → East (input), North → West (output)
        assert_eq!(ports[0].side, Direction::East);
        assert_eq!(ports[0].kind, PortKind::Input);
        assert_eq!(ports[1].side, Direction::West);
        assert_eq!(ports[1].kind, PortKind::Output);
    }

    #[test]
    fn embedder_rotation_east() {
        let ports = rotated_ports(MachineType::Embedder, Direction::East);
        // Canonical: South(in0), West(in1), North(out0)
        // East rotation: South→West, West→North, North→East
        assert_eq!(ports[0].side, Direction::West);
        assert_eq!(ports[0].kind, PortKind::Input);
        assert_eq!(ports[0].slot, 0);
        assert_eq!(ports[1].side, Direction::North);
        assert_eq!(ports[1].kind, PortKind::Input);
        assert_eq!(ports[1].slot, 1);
        assert_eq!(ports[2].side, Direction::East);
        assert_eq!(ports[2].kind, PortKind::Output);
        assert_eq!(ports[2].slot, 0);
    }

    #[test]
    fn all_ports_unique_sides_after_rotation() {
        // Verify no two ports end up on the same side for any rotation.
        for mt in [
            MachineType::Composer,
            MachineType::Inverter,
            MachineType::Embedder,
            MachineType::Quotient,
            MachineType::Transformer,
            MachineType::Source,
        ] {
            for dir in [
                Direction::North,
                Direction::East,
                Direction::South,
                Direction::West,
            ] {
                let ports = rotated_ports(mt, dir);
                let sides: Vec<Direction> = ports.iter().map(|p| p.side).collect();
                for (i, a) in sides.iter().enumerate() {
                    for (j, b) in sides.iter().enumerate() {
                        if i != j {
                            assert_ne!(
                                a, b,
                                "{:?} facing {:?}: duplicate port side {:?}",
                                mt, dir, a
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn port_on_side_finds_correct_port() {
        // Composer facing North: input on South, output on North
        let input = port_on_side(MachineType::Composer, Direction::North, Direction::South);
        assert!(input.is_some());
        assert_eq!(input.unwrap().kind, PortKind::Input);

        let output = port_on_side(MachineType::Composer, Direction::North, Direction::North);
        assert!(output.is_some());
        assert_eq!(output.unwrap().kind, PortKind::Output);

        // No port on East or West for Composer
        assert!(port_on_side(MachineType::Composer, Direction::North, Direction::East).is_none());
        assert!(port_on_side(MachineType::Composer, Direction::North, Direction::West).is_none());
    }

    #[test]
    fn port_on_side_respects_rotation() {
        // Composer facing East: input on West, output on East
        let input = port_on_side(MachineType::Composer, Direction::East, Direction::West);
        assert!(input.is_some());
        assert_eq!(input.unwrap().kind, PortKind::Input);

        let output = port_on_side(MachineType::Composer, Direction::East, Direction::East);
        assert!(output.is_some());
        assert_eq!(output.unwrap().kind, PortKind::Output);
    }

    #[test]
    fn belt_compatibility_input_port() {
        // Input port on South side: belt must flow North (toward machine)
        let port = RotatedPort {
            side: Direction::South,
            kind: PortKind::Input,
            slot: 0,
            cell_offset: (0, 0),
        };
        assert!(belt_compatible_with_port(&port, Direction::North));
        assert!(!belt_compatible_with_port(&port, Direction::South));
        assert!(!belt_compatible_with_port(&port, Direction::East));
        assert!(!belt_compatible_with_port(&port, Direction::West));
    }

    #[test]
    fn belt_compatibility_output_port() {
        // Output port on North side: belt must flow North (away from machine)
        let port = RotatedPort {
            side: Direction::North,
            kind: PortKind::Output,
            slot: 0,
            cell_offset: (0, 0),
        };
        assert!(belt_compatible_with_port(&port, Direction::North));
        assert!(!belt_compatible_with_port(&port, Direction::South));
        assert!(!belt_compatible_with_port(&port, Direction::East));
        assert!(!belt_compatible_with_port(&port, Direction::West));
    }

    #[test]
    fn slot_indices_are_distinct_per_kind() {
        // For multi-port machines, verify input slots don't collide
        // and output slots don't collide.
        for mt in [MachineType::Embedder, MachineType::Quotient, MachineType::Transformer] {
            let ports = port_layout(mt);
            let input_slots: Vec<usize> = ports
                .iter()
                .filter(|p| p.kind == PortKind::Input)
                .map(|p| p.slot)
                .collect();
            let output_slots: Vec<usize> = ports
                .iter()
                .filter(|p| p.kind == PortKind::Output)
                .map(|p| p.slot)
                .collect();
            // No duplicate input slots
            for (i, a) in input_slots.iter().enumerate() {
                for (j, b) in input_slots.iter().enumerate() {
                    if i != j {
                        assert_ne!(a, b, "{:?}: duplicate input slot {}", mt, a);
                    }
                }
            }
            // No duplicate output slots
            for (i, a) in output_slots.iter().enumerate() {
                for (j, b) in output_slots.iter().enumerate() {
                    if i != j {
                        assert_ne!(a, b, "{:?}: duplicate output slot {}", mt, a);
                    }
                }
            }
        }
    }

    #[test]
    fn cell_offsets_within_footprint() {
        // Verify every port's cell_offset is within the machine's footprint bounds.
        for mt in [
            MachineType::Composer,
            MachineType::Inverter,
            MachineType::Embedder,
            MachineType::Quotient,
            MachineType::Transformer,
            MachineType::Source,
        ] {
            let (w, h) = mt.footprint();
            for port in port_layout(mt) {
                let (cx, cy) = port.cell_offset;
                assert!(
                    cx >= 0 && cx < w && cy >= 0 && cy < h,
                    "{:?} port {:?} cell_offset ({}, {}) out of footprint ({}, {})",
                    mt, port.side, cx, cy, w, h,
                );
            }
        }
    }

    #[test]
    fn cell_offsets_on_correct_edge() {
        // Verify each port's cell_offset is on the edge matching its side direction.
        for mt in [
            MachineType::Composer,
            MachineType::Inverter,
            MachineType::Embedder,
            MachineType::Quotient,
            MachineType::Transformer,
            MachineType::Source,
        ] {
            let (w, h) = mt.footprint();
            for port in port_layout(mt) {
                let (cx, cy) = port.cell_offset;
                match port.side {
                    Direction::North => assert_eq!(cy, 0,
                        "{:?} North port should be on y=0, got y={}", mt, cy),
                    Direction::South => assert_eq!(cy, h - 1,
                        "{:?} South port should be on y={}, got y={}", mt, h - 1, cy),
                    Direction::West => assert_eq!(cx, 0,
                        "{:?} West port should be on x=0, got x={}", mt, cx),
                    Direction::East => assert_eq!(cx, w - 1,
                        "{:?} East port should be on x={}, got x={}", mt, w - 1, cx),
                }
            }
        }
    }

    #[test]
    fn no_duplicate_port_cells() {
        // No two ports should occupy the same cell with the same side direction.
        for mt in [
            MachineType::Composer,
            MachineType::Inverter,
            MachineType::Embedder,
            MachineType::Quotient,
            MachineType::Transformer,
            MachineType::Source,
        ] {
            let ports = port_layout(mt);
            for (i, a) in ports.iter().enumerate() {
                for (j, b) in ports.iter().enumerate() {
                    if i != j && a.cell_offset == b.cell_offset && a.side == b.side {
                        panic!(
                            "{:?}: ports {} and {} share cell {:?} side {:?}",
                            mt, i, j, a.cell_offset, a.side,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn port_at_cell_on_side_finds_port() {
        // Inverter facing North: input at (1,1) South, output at (1,0) North
        let port = port_at_cell_on_side(MachineType::Inverter, Direction::North, (1, 1), Direction::South);
        assert!(port.is_some());
        assert_eq!(port.unwrap().kind, PortKind::Input);

        let port = port_at_cell_on_side(MachineType::Inverter, Direction::North, (1, 0), Direction::North);
        assert!(port.is_some());
        assert_eq!(port.unwrap().kind, PortKind::Output);
    }

    #[test]
    fn port_at_cell_on_side_rejects_wrong_cell() {
        // Inverter facing North: no port at (0, 1) South (port is at (1, 1) South)
        let port = port_at_cell_on_side(MachineType::Inverter, Direction::North, (0, 1), Direction::South);
        assert!(port.is_none());
    }

    #[test]
    fn port_at_cell_on_side_with_rotation() {
        // Inverter facing East (2×3 footprint):
        //   input at (0,1) West, output at (1,1) East
        let port = port_at_cell_on_side(MachineType::Inverter, Direction::East, (0, 1), Direction::West);
        assert!(port.is_some());
        assert_eq!(port.unwrap().kind, PortKind::Input);

        let port = port_at_cell_on_side(MachineType::Inverter, Direction::East, (1, 1), Direction::East);
        assert!(port.is_some());
        assert_eq!(port.unwrap().kind, PortKind::Output);

        // Old canonical offset (1,1) with South should not match when facing East
        let port = port_at_cell_on_side(MachineType::Inverter, Direction::East, (1, 1), Direction::South);
        assert!(port.is_none());
    }

    #[test]
    fn rotated_ports_rotate_cell_offsets() {
        // Inverter (3×2) facing East: footprint becomes (2, 3).
        // Canonical input at (1,1) South → rotated to (0,1) West.
        // Canonical output at (1,0) North → rotated to (1,1) East.
        let ports = rotated_ports(MachineType::Inverter, Direction::East);
        let input = ports.iter().find(|p| p.kind == PortKind::Input).unwrap();
        assert_eq!(input.cell_offset, (0, 1));
        assert_eq!(input.side, Direction::West);
        let output = ports.iter().find(|p| p.kind == PortKind::Output).unwrap();
        assert_eq!(output.cell_offset, (1, 1));
        assert_eq!(output.side, Direction::East);
    }

    #[test]
    fn rotated_ports_south_cell_offsets() {
        // Inverter (3×2) facing South: footprint stays (3, 2).
        // Canonical input at (1,1) South → rotated to (1,0) North.
        // Canonical output at (1,0) North → rotated to (1,1) South.
        let ports = rotated_ports(MachineType::Inverter, Direction::South);
        let input = ports.iter().find(|p| p.kind == PortKind::Input).unwrap();
        assert_eq!(input.cell_offset, (1, 0));
        assert_eq!(input.side, Direction::North);
        let output = ports.iter().find(|p| p.kind == PortKind::Output).unwrap();
        assert_eq!(output.cell_offset, (1, 1));
        assert_eq!(output.side, Direction::South);
    }

    #[test]
    fn rotated_ports_west_cell_offsets() {
        // Inverter (3×2) facing West: footprint becomes (2, 3).
        // Canonical input at (1,1) South → rotated to (1,1) East.
        // Canonical output at (1,0) North → rotated to (0,1) West.
        let ports = rotated_ports(MachineType::Inverter, Direction::West);
        let input = ports.iter().find(|p| p.kind == PortKind::Input).unwrap();
        assert_eq!(input.cell_offset, (1, 1));
        assert_eq!(input.side, Direction::East);
        let output = ports.iter().find(|p| p.kind == PortKind::Output).unwrap();
        assert_eq!(output.cell_offset, (0, 1));
        assert_eq!(output.side, Direction::West);
    }

    #[test]
    fn rotated_cell_offsets_on_correct_edge() {
        // After rotation, each port's cell_offset must sit on the edge
        // corresponding to its rotated side within the rotated footprint.
        for mt in [
            MachineType::Composer,
            MachineType::Inverter,
            MachineType::Embedder,
            MachineType::Quotient,
            MachineType::Transformer,
            MachineType::Source,
        ] {
            for dir in [
                Direction::North,
                Direction::East,
                Direction::South,
                Direction::West,
            ] {
                let (rw, rh) = dir.rotate_footprint(mt.footprint().0, mt.footprint().1);
                for port in rotated_ports(mt, dir) {
                    let (cx, cy) = port.cell_offset;
                    match port.side {
                        Direction::North => assert_eq!(cy, 0,
                            "{:?} facing {:?}: North port at ({},{}) should have y=0", mt, dir, cx, cy),
                        Direction::South => assert_eq!(cy, rh - 1,
                            "{:?} facing {:?}: South port at ({},{}) should have y={}", mt, dir, cx, cy, rh - 1),
                        Direction::West => assert_eq!(cx, 0,
                            "{:?} facing {:?}: West port at ({},{}) should have x=0", mt, dir, cx, cy),
                        Direction::East => assert_eq!(cx, rw - 1,
                            "{:?} facing {:?}: East port at ({},{}) should have x={}", mt, dir, cx, cy, rw - 1),
                    }
                }
            }
        }
    }
}
