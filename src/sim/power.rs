use std::collections::HashMap;

use crate::game::world::EntityId;
use crate::hyperbolic::tiling::TileAddr;

/// Power connection radius in grid squares.
pub const POWER_RADIUS: f32 = 8.0;

/// Power production rate for a Quadrupole.
pub const QUADRUPOLE_RATE: f32 = 2.0;

/// Power production rate for a Dynamo.
pub const DYNAMO_RATE: f32 = 8.0;

/// Power consumption rate for a machine.
pub const MACHINE_CONSUMPTION: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerNodeKind {
    Producer,
    Consumer,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PowerNode {
    pub entity: EntityId,
    pub kind: PowerNodeKind,
    pub rate: f32,
    pub tile: TileAddr,
    pub gx: i16,
    pub gy: i16,
    /// Whether this node is exempt from power requirements (e.g. Source machines).
    pub exempt: bool,
}

/// Power network: tracks all power-relevant entities, builds a connection
/// graph based on proximity, and solves connected-component ratio-based
/// power distribution each tick.
pub struct PowerNetwork {
    nodes: Vec<PowerNode>,
    entity_to_idx: HashMap<EntityId, usize>,
    /// Per-node power satisfaction [0.0 .. 1.0].
    satisfaction: Vec<f32>,
    /// Adjacency list (rebuilt when dirty).
    adjacency: Vec<Vec<usize>>,
    /// Whether the graph needs rebuilding.
    dirty: bool,
}

impl PowerNetwork {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            entity_to_idx: HashMap::new(),
            satisfaction: Vec::new(),
            adjacency: Vec::new(),
            dirty: false,
        }
    }

    /// Register a power node (producer or consumer).
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        entity: EntityId,
        kind: PowerNodeKind,
        rate: f32,
        tile: &[u8],
        gx: i16,
        gy: i16,
        exempt: bool,
    ) {
        let idx = self.nodes.len();
        self.nodes.push(PowerNode {
            entity,
            kind,
            rate,
            tile: TileAddr::from_slice(tile),
            gx,
            gy,
            exempt,
        });
        self.entity_to_idx.insert(entity, idx);
        self.satisfaction.push(if exempt { 1.0 } else { 0.0 });
        self.dirty = true;
    }

    /// Remove a power node by entity ID.
    #[allow(dead_code)]
    pub fn remove(&mut self, entity: EntityId) -> bool {
        let Some(idx) = self.entity_to_idx.remove(&entity) else {
            return false;
        };
        let last = self.nodes.len() - 1;

        if idx != last {
            self.nodes.swap(idx, last);
            self.satisfaction.swap(idx, last);

            let swapped_entity = self.nodes[idx].entity;
            self.entity_to_idx.insert(swapped_entity, idx);
        }

        self.nodes.pop();
        self.satisfaction.pop();
        self.dirty = true;
        true
    }

    /// Get power satisfaction for an entity [0.0 .. 1.0].
    pub fn satisfaction(&self, entity: EntityId) -> Option<f32> {
        self.entity_to_idx
            .get(&entity)
            .map(|&i| self.satisfaction[i])
    }

    /// Number of registered nodes.
    #[allow(dead_code)]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Rebuild the adjacency graph based on proximity within same tile.
    fn rebuild_connections(&mut self) {
        let n = self.nodes.len();
        self.adjacency.clear();
        self.adjacency.resize(n, Vec::new());

        let radius_sq = POWER_RADIUS * POWER_RADIUS;

        for i in 0..n {
            for j in (i + 1)..n {
                // Only connect nodes in the same tile
                if self.nodes[i].tile != self.nodes[j].tile {
                    continue;
                }
                let dx = (self.nodes[i].gx - self.nodes[j].gx) as f32;
                let dy = (self.nodes[i].gy - self.nodes[j].gy) as f32;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq <= radius_sq {
                    self.adjacency[i].push(j);
                    self.adjacency[j].push(i);
                }
            }
        }

        self.dirty = false;
    }

    /// Solve power distribution: BFS connected components, ratio-based.
    /// Updates satisfaction for all nodes.
    pub fn solve(&mut self) {
        if self.dirty {
            self.rebuild_connections();
        }

        let n = self.nodes.len();
        if n == 0 {
            return;
        }

        let mut visited = vec![false; n];

        for start in 0..n {
            if visited[start] {
                continue;
            }

            // BFS to find connected component
            let mut component = Vec::new();
            let mut queue = std::collections::VecDeque::new();
            queue.push_back(start);
            visited[start] = true;

            while let Some(node) = queue.pop_front() {
                component.push(node);
                for &neighbor in &self.adjacency[node] {
                    if !visited[neighbor] {
                        visited[neighbor] = true;
                        queue.push_back(neighbor);
                    }
                }
            }

            // Calculate ratio for this component
            let mut total_production = 0.0f32;
            let mut total_consumption = 0.0f32;

            for &idx in &component {
                match self.nodes[idx].kind {
                    PowerNodeKind::Producer => total_production += self.nodes[idx].rate,
                    PowerNodeKind::Consumer => {
                        if !self.nodes[idx].exempt {
                            total_consumption += self.nodes[idx].rate;
                        }
                    }
                }
            }

            let ratio = if total_consumption > 0.0 {
                (total_production / total_consumption).min(1.0)
            } else {
                1.0 // no demand = fully satisfied
            };

            // Apply satisfaction to all nodes in the component
            for &idx in &component {
                self.satisfaction[idx] = if self.nodes[idx].exempt {
                    1.0
                } else {
                    ratio
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;

    fn make_entities(n: usize) -> (SlotMap<EntityId, ()>, Vec<EntityId>) {
        let mut sm = SlotMap::with_key();
        let ids: Vec<EntityId> = (0..n).map(|_| sm.insert(())).collect();
        (sm, ids)
    }

    #[test]
    fn empty_network() {
        let mut net = PowerNetwork::new();
        net.solve();
        assert_eq!(net.node_count(), 0);
    }

    #[test]
    fn single_producer_no_consumers() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(1);
        net.add(ids[0], PowerNodeKind::Producer, QUADRUPOLE_RATE, &[0], 0, 0, false);
        net.solve();
        assert_eq!(net.satisfaction(ids[0]), Some(1.0));
    }

    #[test]
    fn single_consumer_no_producer() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(1);
        net.add(ids[0], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 0, 0, false);
        net.solve();
        assert_eq!(net.satisfaction(ids[0]), Some(0.0));
    }

    #[test]
    fn one_quad_two_machines_full_power() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(3);
        // Quadrupole at (0,0), rate 2.0
        net.add(ids[0], PowerNodeKind::Producer, QUADRUPOLE_RATE, &[0], 0, 0, false);
        // Machine at (3,0), rate 1.0
        net.add(ids[1], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 3, 0, false);
        // Machine at (0,3), rate 1.0
        net.add(ids[2], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 0, 3, false);
        net.solve();
        // 2.0 production / 2.0 consumption = 1.0
        assert_eq!(net.satisfaction(ids[1]), Some(1.0));
        assert_eq!(net.satisfaction(ids[2]), Some(1.0));
    }

    #[test]
    fn overloaded_power() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(4);
        // Quadrupole at (0,0), rate 2.0
        net.add(ids[0], PowerNodeKind::Producer, QUADRUPOLE_RATE, &[0], 0, 0, false);
        // 3 machines nearby, total consumption 3.0
        net.add(ids[1], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 1, 0, false);
        net.add(ids[2], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 0, 1, false);
        net.add(ids[3], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 1, 1, false);
        net.solve();
        // 2.0 / 3.0 = 0.6667
        let expected = 2.0 / 3.0;
        let sat = net.satisfaction(ids[1]).unwrap();
        assert!((sat - expected).abs() < 0.001, "expected {}, got {}", expected, sat);
        assert!((net.satisfaction(ids[2]).unwrap() - expected).abs() < 0.001);
        assert!((net.satisfaction(ids[3]).unwrap() - expected).abs() < 0.001);
    }

    #[test]
    fn out_of_range_not_connected() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(2);
        // Quadrupole at (0,0)
        net.add(ids[0], PowerNodeKind::Producer, QUADRUPOLE_RATE, &[0], 0, 0, false);
        // Machine at (20,20) — way out of range
        net.add(ids[1], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 20, 20, false);
        net.solve();
        // Machine not connected to any producer
        assert_eq!(net.satisfaction(ids[1]), Some(0.0));
    }

    #[test]
    fn different_tiles_not_connected() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(2);
        // Quadrupole in tile [0]
        net.add(ids[0], PowerNodeKind::Producer, QUADRUPOLE_RATE, &[0], 0, 0, false);
        // Machine in tile [1], same grid coords but different tile
        net.add(ids[1], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[1], 0, 0, false);
        net.solve();
        assert_eq!(net.satisfaction(ids[1]), Some(0.0));
    }

    #[test]
    fn exempt_consumer_always_full() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(2);
        // No producer. Exempt consumer (Source machine).
        net.add(ids[0], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 0, 0, true);
        // Non-exempt consumer
        net.add(ids[1], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 1, 0, false);
        net.solve();
        assert_eq!(net.satisfaction(ids[0]), Some(1.0)); // exempt
        assert_eq!(net.satisfaction(ids[1]), Some(0.0)); // no power
    }

    #[test]
    fn exempt_not_counted_in_consumption() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(3);
        // Quadrupole, rate 2.0
        net.add(ids[0], PowerNodeKind::Producer, QUADRUPOLE_RATE, &[0], 0, 0, false);
        // Exempt consumer (should not count toward consumption)
        net.add(ids[1], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 1, 0, true);
        // Regular consumer, rate 1.0
        net.add(ids[2], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 2, 0, false);
        net.solve();
        // 2.0 production / 1.0 consumption = 1.0 (exempt doesn't count)
        assert_eq!(net.satisfaction(ids[1]), Some(1.0)); // exempt
        assert_eq!(net.satisfaction(ids[2]), Some(1.0)); // fully powered
    }

    #[test]
    fn dynamo_higher_rate() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(9);
        // Dynamo at center, rate 8.0
        net.add(ids[0], PowerNodeKind::Producer, DYNAMO_RATE, &[0], 4, 4, false);
        // 8 machines around it
        for i in 1..=8 {
            let gx = 4 + ((i as i16 - 1) % 3) - 1;
            let gy = 4 + ((i as i16 - 1) / 3) - 1;
            net.add(ids[i], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], gx, gy, false);
        }
        net.solve();
        // 8.0 / 8.0 = 1.0
        for i in 1..=8 {
            assert_eq!(net.satisfaction(ids[i]), Some(1.0));
        }
    }

    #[test]
    fn remove_node() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(3);
        net.add(ids[0], PowerNodeKind::Producer, QUADRUPOLE_RATE, &[0], 0, 0, false);
        net.add(ids[1], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 1, 0, false);
        net.add(ids[2], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 2, 0, false);

        assert!(net.remove(ids[1]));
        assert_eq!(net.node_count(), 2);

        net.solve();
        // 2.0 / 1.0 = 1.0 (only one consumer left)
        assert_eq!(net.satisfaction(ids[2]), Some(1.0));
        assert_eq!(net.satisfaction(ids[1]), None); // removed
    }

    #[test]
    fn two_separate_components() {
        let mut net = PowerNetwork::new();
        let (_sm, ids) = make_entities(7);
        // Component 1: well-powered (at gx=0)
        net.add(ids[0], PowerNodeKind::Producer, QUADRUPOLE_RATE, &[0], 0, 0, false);
        net.add(ids[1], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 1, 0, false);
        // Component 2: underpowered (at gx=50, far away)
        net.add(ids[2], PowerNodeKind::Producer, QUADRUPOLE_RATE, &[0], 50, 0, false);
        net.add(ids[3], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 51, 0, false);

        // Add extra consumers to component 2
        for i in 0..3 {
            net.add(ids[4 + i], PowerNodeKind::Consumer, MACHINE_CONSUMPTION, &[0], 52 + i as i16, 0, false);
        }

        net.solve();
        // Component 1: 2.0/1.0 = 1.0
        assert_eq!(net.satisfaction(ids[1]), Some(1.0));
        // Component 2: 2.0/4.0 = 0.5
        let sat = net.satisfaction(ids[3]).unwrap();
        assert!((sat - 0.5).abs() < 0.001, "expected 0.5, got {}", sat);
    }
}
