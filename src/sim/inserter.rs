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
}

/// Get the canonical port layout for a machine type (defined facing North).
#[allow(dead_code)]
pub fn port_layout(machine_type: MachineType) -> &'static [PortDef] {
    use Direction::*;
    use PortKind::*;
    match machine_type {
        MachineType::Composer => &[
            PortDef { side: South, kind: Input, slot: 0 },
            PortDef { side: North, kind: Output, slot: 0 },
        ],
        MachineType::Inverter => &[
            PortDef { side: South, kind: Input, slot: 0 },
            PortDef { side: North, kind: Output, slot: 0 },
        ],
        MachineType::Embedder => &[
            PortDef { side: South, kind: Input, slot: 0 },
            PortDef { side: West, kind: Input, slot: 1 },
            PortDef { side: North, kind: Output, slot: 0 },
        ],
        MachineType::Quotient => &[
            PortDef { side: South, kind: Input, slot: 0 },
            PortDef { side: North, kind: Output, slot: 0 },
            PortDef { side: East, kind: Output, slot: 1 },
        ],
        MachineType::Transformer => &[
            PortDef { side: South, kind: Input, slot: 0 },
            PortDef { side: West, kind: Input, slot: 1 },
            PortDef { side: North, kind: Output, slot: 0 },
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
}

/// Get the rotated port layout for a machine at the given facing direction.
#[allow(dead_code)]
pub fn rotated_ports(machine_type: MachineType, facing: Direction) -> Vec<RotatedPort> {
    let n = facing.rotations_from_north();
    port_layout(machine_type)
        .iter()
        .map(|def| RotatedPort {
            side: def.side.rotate_n_cw(n),
            kind: def.kind,
            slot: def.slot,
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
}
