use std::collections::HashMap;

use crate::game::world::{Direction, EntityId, WorldState};
use crate::sim::belt::BeltNetwork;

/// Bit shift for encoding a direction in the connection bitmask (2 bits per side).
fn side_shift(dir: Direction) -> u8 {
    match dir {
        Direction::North => 0,
        Direction::East => 2,
        Direction::South => 4,
        Direction::West => 6,
    }
}

/// How a splitter behaves, auto-detected from connected belt directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitterMode {
    /// No inputs or no outputs connected.
    Inactive,
    /// Multiple inputs, one output — items merge round-robin.
    Merger,
    /// One input, multiple outputs — items split round-robin.
    Splitter,
    /// Two inputs, two outputs — balanced distribution.
    Balancer,
}

/// Per-splitter state.
#[derive(Clone, Debug)]
pub struct SplitterState {
    pub entity: EntityId,
    /// Belt entities feeding items into this splitter (output_end = Splitter).
    pub inputs: Vec<EntityId>,
    /// Belt entities receiving items from this splitter (input_end = Splitter).
    pub outputs: Vec<EntityId>,
    /// Auto-detected operating mode.
    pub mode: SplitterMode,
    /// Round-robin index for fair distribution across outputs (or inputs for merger).
    pub round_robin_idx: usize,
}

/// Pool of all placed splitters. Dense storage indexed by EntityId.
pub struct SplitterPool {
    splitters: Vec<SplitterState>,
    entity_to_idx: HashMap<EntityId, usize>,
}

impl SplitterPool {
    pub fn new() -> Self {
        Self {
            splitters: Vec::new(),
            entity_to_idx: HashMap::new(),
        }
    }

    /// Number of active splitters.
    pub fn count(&self) -> usize {
        self.splitters.len()
    }

    /// Register a newly placed splitter. Returns the dense index.
    pub fn add(&mut self, entity: EntityId) -> usize {
        let idx = self.splitters.len();
        self.splitters.push(SplitterState {
            entity,
            inputs: Vec::new(),
            outputs: Vec::new(),
            mode: SplitterMode::Inactive,
            round_robin_idx: 0,
        });
        self.entity_to_idx.insert(entity, idx);
        idx
    }

    /// Remove a splitter by EntityId. Swap-removes with the last element.
    pub fn remove(&mut self, entity: EntityId) -> bool {
        let Some(idx) = self.entity_to_idx.remove(&entity) else {
            return false;
        };
        let last = self.splitters.len() - 1;

        if idx != last {
            self.splitters.swap(idx, last);
            let swapped_entity = self.splitters[idx].entity;
            self.entity_to_idx.insert(swapped_entity, idx);
        }

        self.splitters.pop();
        true
    }

    /// Look up the dense index for an EntityId.
    pub fn index_of(&self, entity: EntityId) -> Option<usize> {
        self.entity_to_idx.get(&entity).copied()
    }

    /// Get a reference to the splitter state for an entity.
    pub fn get(&self, entity: EntityId) -> Option<&SplitterState> {
        self.index_of(entity).map(|i| &self.splitters[i])
    }

    /// Get a mutable reference to the splitter state for an entity.
    pub fn get_mut(&mut self, entity: EntityId) -> Option<&mut SplitterState> {
        let i = self.index_of(entity)?;
        Some(&mut self.splitters[i])
    }

    /// Re-detect the operating mode based on current input/output counts.
    pub fn detect_mode(&mut self, entity: EntityId) {
        let Some(i) = self.index_of(entity) else { return };
        let s = &mut self.splitters[i];
        let ni = s.inputs.len();
        let no = s.outputs.len();
        s.mode = match (ni, no) {
            (0, _) | (_, 0) => SplitterMode::Inactive,
            (1, 1) => SplitterMode::Splitter, // pass-through acts like splitter
            (_, 1) => SplitterMode::Merger,
            (1, _) => SplitterMode::Splitter,
            (2, 2) => SplitterMode::Balancer,
            _ => SplitterMode::Splitter, // mixed: treat as splitter on outputs
        };
    }

    /// Register a belt entity as an input to this splitter.
    pub fn add_input(&mut self, splitter: EntityId, belt: EntityId) {
        if let Some(s) = self.get_mut(splitter) {
            if !s.inputs.contains(&belt) {
                s.inputs.push(belt);
            }
        }
    }

    /// Register a belt entity as an output from this splitter.
    pub fn add_output(&mut self, splitter: EntityId, belt: EntityId) {
        if let Some(s) = self.get_mut(splitter) {
            if !s.outputs.contains(&belt) {
                s.outputs.push(belt);
            }
        }
    }

    /// Compute a bitmask encoding which sides of the splitter have input/output belts.
    /// 2 bits per direction: 0=none, 1=input, 2=output.
    /// Packed as: north | (east << 2) | (south << 4) | (west << 6).
    pub fn connection_bitmask(&self, splitter: EntityId, world: &WorldState) -> u8 {
        let Some(s) = self.get(splitter) else { return 0 };
        let Some(spos) = world.position(splitter) else { return 0 };
        let (sx, sy) = (spos.gx as i32, spos.gy as i32);

        let mut mask: u8 = 0;
        // Determine which side each connected belt is on
        for &belt in &s.inputs {
            if let Some(side) = Self::belt_side(belt, sx, sy, world) {
                let shift = side_shift(side);
                mask |= 1 << shift; // 1 = input
            }
        }
        for &belt in &s.outputs {
            if let Some(side) = Self::belt_side(belt, sx, sy, world) {
                let shift = side_shift(side);
                mask |= 2 << shift; // 2 = output
            }
        }
        mask
    }

    /// Determine which side of the splitter a belt entity is on.
    fn belt_side(belt: EntityId, sx: i32, sy: i32, world: &WorldState) -> Option<Direction> {
        let bpos = world.position(belt)?;
        let (bx, by) = (bpos.gx as i32, bpos.gy as i32);
        let (dx, dy) = (bx - sx, by - sy);
        match (dx, dy) {
            (0, -1) => Some(Direction::North),
            (1, 0) => Some(Direction::East),
            (0, 1) => Some(Direction::South),
            (-1, 0) => Some(Direction::West),
            _ => None,
        }
    }

    /// Remove a belt entity from this splitter's inputs and outputs.
    pub fn disconnect_belt(&mut self, splitter: EntityId, belt: EntityId) {
        if let Some(s) = self.get_mut(splitter) {
            s.inputs.retain(|&e| e != belt);
            s.outputs.retain(|&e| e != belt);
        }
    }

    /// Run one simulation tick for all splitters.
    /// Transfers items between input and output belts based on each splitter's mode.
    /// Called each tick after belt advance.
    pub fn tick(&mut self, belt_network: &mut BeltNetwork) {
        for splitter in &mut self.splitters {
            match splitter.mode {
                SplitterMode::Inactive => {}
                SplitterMode::Merger => {
                    // Round-robin pull from input lines, push to single output.
                    let ni = splitter.inputs.len();
                    if ni == 0 || splitter.outputs.is_empty() {
                        continue;
                    }
                    let output_belt = splitter.outputs[0];
                    if !belt_network.can_accept_at_entity_input(output_belt) {
                        continue;
                    }
                    let start = splitter.round_robin_idx % ni;
                    for attempt in 0..ni {
                        let idx = (start + attempt) % ni;
                        let input_belt = splitter.inputs[idx];
                        if let Some(item) = belt_network.take_front_item(input_belt) {
                            belt_network.push_to_entity_input(output_belt, item);
                            // Advance based on intended input, not fallback,
                            // so each input gets fair priority.
                            splitter.round_robin_idx = start + 1;
                            break;
                        }
                    }
                }
                SplitterMode::Splitter => {
                    // Take from single input, round-robin push to outputs.
                    let no = splitter.outputs.len();
                    if splitter.inputs.is_empty() || no == 0 {
                        continue;
                    }
                    let input_belt = splitter.inputs[0];
                    let item = match belt_network.peek_front_item(input_belt) {
                        Some(it) => it,
                        None => continue,
                    };
                    let start = splitter.round_robin_idx % no;
                    for attempt in 0..no {
                        let idx = (start + attempt) % no;
                        let output_belt = splitter.outputs[idx];
                        if belt_network.can_accept_at_entity_input(output_belt) {
                            belt_network.take_front_item(input_belt);
                            belt_network.push_to_entity_input(output_belt, item);
                            // Advance based on intended output, not fallback,
                            // so items alternate evenly across outputs.
                            splitter.round_robin_idx = start + 1;
                            break;
                        }
                    }
                }
                SplitterMode::Balancer => {
                    // Each input independently tries its corresponding output, with overflow.
                    let ni = splitter.inputs.len();
                    let no = splitter.outputs.len();
                    if ni == 0 || no == 0 {
                        continue;
                    }
                    for i in 0..ni {
                        let input_belt = splitter.inputs[i];
                        let item = match belt_network.peek_front_item(input_belt) {
                            Some(it) => it,
                            None => continue,
                        };
                        // Primary: corresponding output
                        let primary = i % no;
                        if belt_network.can_accept_at_entity_input(splitter.outputs[primary]) {
                            belt_network.take_front_item(input_belt);
                            belt_network.push_to_entity_input(splitter.outputs[primary], item);
                            continue;
                        }
                        // Overflow: try other outputs
                        let mut transferred = false;
                        for overflow in 1..no {
                            let alt = (primary + overflow) % no;
                            if belt_network.can_accept_at_entity_input(splitter.outputs[alt]) {
                                belt_network.take_front_item(input_belt);
                                belt_network.push_to_entity_input(splitter.outputs[alt], item);
                                transferred = true;
                                break;
                            }
                        }
                        let _ = transferred;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;
    use crate::game::items::ItemId;
    use crate::game::world::{Direction, WorldState};
    use crate::sim::belt::BeltNetwork;

    fn make_entity() -> (SlotMap<EntityId, ()>, EntityId) {
        let mut sm = SlotMap::with_key();
        let id = sm.insert(());
        (sm, id)
    }

    /// Place a belt entity and register it with the belt network.
    fn place_belt(world: &mut WorldState, net: &mut BeltNetwork, addr: &[u8], gx: i32, gy: i32, dir: Direction) -> EntityId {
        let entity = world.place(addr, (gx, gy), ItemId::Belt, dir).unwrap();
        net.on_belt_placed(entity, addr, gx, gy, dir, world);
        entity
    }

    /// Advance belt ticks until all items on the entity's line reach pos=0
    /// (or max 200 ticks to avoid infinite loops).
    fn advance_to_output(net: &mut BeltNetwork, _entity: EntityId) {
        for _ in 0..200 {
            net.tick();
        }
    }

    /// Count items on a belt entity's segment.
    fn item_count(net: &BeltNetwork, entity: EntityId) -> usize {
        net.entity_items(entity).map(|(items, _)| items.len()).unwrap_or(0)
    }

    /// Set up a merger: 2 input belts → splitter → 1 output belt.
    /// Returns (pool, input_belt_1, input_belt_2, output_belt, splitter_entity).
    fn setup_merger(world: &mut WorldState, net: &mut BeltNetwork) -> (SplitterPool, EntityId, EntityId, EntityId, EntityId) {
        let addr: &[u8] = &[0];
        // Splitter at (5, 5). Input belts point toward it, output belt points away.
        let splitter_entity = world.place(addr, (5, 5), ItemId::Splitter, Direction::North).unwrap();

        // Input belt 1: going East at (4, 5), output faces splitter at (5, 5)
        let in1 = place_belt(world, net, addr, 4, 5, Direction::East);
        // Input belt 2: going South at (5, 4), output faces splitter at (5, 5)
        let in2 = place_belt(world, net, addr, 5, 4, Direction::South);
        // Output belt: going East at (6, 5), input faces splitter at (5, 5)
        let out1 = place_belt(world, net, addr, 6, 5, Direction::East);

        // Wire connections
        net.connect_belt_to_splitter(in1, splitter_entity);
        net.connect_belt_to_splitter(in2, splitter_entity);
        net.connect_splitter_to_belt(out1, splitter_entity);

        let mut pool = SplitterPool::new();
        pool.add(splitter_entity);
        pool.add_input(splitter_entity, in1);
        pool.add_input(splitter_entity, in2);
        pool.add_output(splitter_entity, out1);
        pool.detect_mode(splitter_entity);

        assert_eq!(pool.get(splitter_entity).unwrap().mode, SplitterMode::Merger);

        (pool, in1, in2, out1, splitter_entity)
    }

    /// Set up a splitter: 1 input belt → splitter → 2 output belts.
    fn setup_splitter(world: &mut WorldState, net: &mut BeltNetwork) -> (SplitterPool, EntityId, EntityId, EntityId, EntityId) {
        let addr: &[u8] = &[0];
        let splitter_entity = world.place(addr, (5, 5), ItemId::Splitter, Direction::North).unwrap();

        // Input belt: going East at (4, 5)
        let in1 = place_belt(world, net, addr, 4, 5, Direction::East);
        // Output belt 1: going East at (6, 5)
        let out1 = place_belt(world, net, addr, 6, 5, Direction::East);
        // Output belt 2: going South at (5, 6)
        let out2 = place_belt(world, net, addr, 5, 6, Direction::South);

        net.connect_belt_to_splitter(in1, splitter_entity);
        net.connect_splitter_to_belt(out1, splitter_entity);
        net.connect_splitter_to_belt(out2, splitter_entity);

        let mut pool = SplitterPool::new();
        pool.add(splitter_entity);
        pool.add_input(splitter_entity, in1);
        pool.add_output(splitter_entity, out1);
        pool.add_output(splitter_entity, out2);
        pool.detect_mode(splitter_entity);

        assert_eq!(pool.get(splitter_entity).unwrap().mode, SplitterMode::Splitter);

        (pool, in1, out1, out2, splitter_entity)
    }

    /// Set up a balancer: 2 input belts → splitter → 2 output belts.
    fn setup_balancer(world: &mut WorldState, net: &mut BeltNetwork) -> (SplitterPool, EntityId, EntityId, EntityId, EntityId, EntityId) {
        let addr: &[u8] = &[0];
        let splitter_entity = world.place(addr, (5, 5), ItemId::Splitter, Direction::North).unwrap();

        // Input belts
        let in1 = place_belt(world, net, addr, 4, 5, Direction::East);
        let in2 = place_belt(world, net, addr, 5, 4, Direction::South);
        // Output belts
        let out1 = place_belt(world, net, addr, 6, 5, Direction::East);
        let out2 = place_belt(world, net, addr, 5, 6, Direction::South);

        net.connect_belt_to_splitter(in1, splitter_entity);
        net.connect_belt_to_splitter(in2, splitter_entity);
        net.connect_splitter_to_belt(out1, splitter_entity);
        net.connect_splitter_to_belt(out2, splitter_entity);

        let mut pool = SplitterPool::new();
        pool.add(splitter_entity);
        pool.add_input(splitter_entity, in1);
        pool.add_input(splitter_entity, in2);
        pool.add_output(splitter_entity, out1);
        pool.add_output(splitter_entity, out2);
        pool.detect_mode(splitter_entity);

        assert_eq!(pool.get(splitter_entity).unwrap().mode, SplitterMode::Balancer);

        (pool, in1, in2, out1, out2, splitter_entity)
    }

    #[test]
    fn add_and_lookup() {
        let mut pool = SplitterPool::new();
        let (mut sm, e1) = make_entity();
        let e2 = sm.insert(());

        let idx1 = pool.add(e1);
        let idx2 = pool.add(e2);

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(pool.count(), 2);
        assert_eq!(pool.index_of(e1), Some(0));
        assert_eq!(pool.index_of(e2), Some(1));
    }

    #[test]
    fn get_state() {
        let mut pool = SplitterPool::new();
        let (_, e1) = make_entity();
        pool.add(e1);

        let state = pool.get(e1).unwrap();
        assert_eq!(state.entity, e1);
        assert_eq!(state.mode, SplitterMode::Inactive);
        assert!(state.inputs.is_empty());
        assert!(state.outputs.is_empty());
    }

    #[test]
    fn remove_swap() {
        let mut pool = SplitterPool::new();
        let mut sm: SlotMap<EntityId, ()> = SlotMap::with_key();
        let e1 = sm.insert(());
        let e2 = sm.insert(());
        let e3 = sm.insert(());

        pool.add(e1);
        pool.add(e2);
        pool.add(e3);

        // Remove middle element — e3 should swap into index 1
        assert!(pool.remove(e2));
        assert_eq!(pool.count(), 2);
        assert_eq!(pool.index_of(e2), None);
        assert_eq!(pool.index_of(e1), Some(0));
        assert_eq!(pool.index_of(e3), Some(1));
    }

    #[test]
    fn remove_last_no_swap() {
        let mut pool = SplitterPool::new();
        let (mut sm, e1) = make_entity();
        let e2 = sm.insert(());

        pool.add(e1);
        pool.add(e2);

        assert!(pool.remove(e2));
        assert_eq!(pool.count(), 1);
        assert_eq!(pool.index_of(e1), Some(0));
    }

    #[test]
    fn remove_nonexistent() {
        let mut pool = SplitterPool::new();
        let (_, e1) = make_entity();
        assert!(!pool.remove(e1));
    }

    #[test]
    fn detect_mode_inactive() {
        let mut pool = SplitterPool::new();
        let (_, e1) = make_entity();
        pool.add(e1);
        pool.detect_mode(e1);
        assert_eq!(pool.get(e1).unwrap().mode, SplitterMode::Inactive);
    }

    #[test]
    fn detect_mode_merger() {
        let mut pool = SplitterPool::new();
        let (mut sm, e1) = make_entity();
        pool.add(e1);

        // Simulate 2 inputs, 1 output using belt entity IDs
        let b1 = sm.insert(());
        let b2 = sm.insert(());
        let b3 = sm.insert(());
        pool.add_input(e1, b1);
        pool.add_input(e1, b2);
        pool.add_output(e1, b3);

        pool.detect_mode(e1);
        assert_eq!(pool.get(e1).unwrap().mode, SplitterMode::Merger);
    }

    #[test]
    fn detect_mode_splitter() {
        let mut pool = SplitterPool::new();
        let (mut sm, e1) = make_entity();
        pool.add(e1);

        let b1 = sm.insert(());
        let b2 = sm.insert(());
        let b3 = sm.insert(());
        pool.add_input(e1, b1);
        pool.add_output(e1, b2);
        pool.add_output(e1, b3);

        pool.detect_mode(e1);
        assert_eq!(pool.get(e1).unwrap().mode, SplitterMode::Splitter);
    }

    #[test]
    fn detect_mode_balancer() {
        let mut pool = SplitterPool::new();
        let (mut sm, e1) = make_entity();
        pool.add(e1);

        let b1 = sm.insert(());
        let b2 = sm.insert(());
        let b3 = sm.insert(());
        let b4 = sm.insert(());
        pool.add_input(e1, b1);
        pool.add_input(e1, b2);
        pool.add_output(e1, b3);
        pool.add_output(e1, b4);

        pool.detect_mode(e1);
        assert_eq!(pool.get(e1).unwrap().mode, SplitterMode::Balancer);
    }

    #[test]
    fn add_input_no_duplicates() {
        let mut pool = SplitterPool::new();
        let (mut sm, e1) = make_entity();
        pool.add(e1);

        let b1 = sm.insert(());
        pool.add_input(e1, b1);
        pool.add_input(e1, b1); // duplicate
        assert_eq!(pool.get(e1).unwrap().inputs.len(), 1);
    }

    #[test]
    fn disconnect_belt_removes_from_both() {
        let mut pool = SplitterPool::new();
        let (mut sm, e1) = make_entity();
        pool.add(e1);

        let b1 = sm.insert(());
        let b2 = sm.insert(());
        pool.add_input(e1, b1);
        pool.add_output(e1, b2);

        pool.disconnect_belt(e1, b1);
        assert!(pool.get(e1).unwrap().inputs.is_empty());
        assert_eq!(pool.get(e1).unwrap().outputs.len(), 1);

        pool.disconnect_belt(e1, b2);
        assert!(pool.get(e1).unwrap().outputs.is_empty());
    }

    // ── Tick tests ──────────────────────────────────────────────────────

    #[test]
    fn merger_alternates_inputs() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let (mut pool, in1, in2, out1, _) = setup_merger(&mut world, &mut net);

        // Place items on both input belts
        net.spawn_item_on_entity(in1, ItemId::NullSet);
        net.spawn_item_on_entity(in2, ItemId::Point);

        // Advance items to pos=0
        advance_to_output(&mut net, in1);

        // Splitter tick 1: should take from first input (in1)
        pool.tick(&mut net);
        assert_eq!(item_count(&net, in1), 0);
        assert_eq!(item_count(&net, in2), 1); // still waiting

        // Advance belt ticks so out1's item moves away from input end,
        // making room for the next transfer (need MIN_ITEM_GAP=64 clearance,
        // at speed 4 that's 16+ ticks)
        for _ in 0..20 {
            net.tick();
        }

        // Splitter tick 2: should take from second input (in2), not in1 again
        pool.tick(&mut net);
        assert_eq!(item_count(&net, in2), 0);
    }

    #[test]
    fn splitter_distributes_round_robin() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let (mut pool, in1, out1, out2, _) = setup_splitter(&mut world, &mut net);

        // Place item on input
        net.spawn_item_on_entity(in1, ItemId::NullSet);
        advance_to_output(&mut net, in1);

        // Tick 1: should go to first output (out1)
        pool.tick(&mut net);
        assert_eq!(item_count(&net, in1), 0);
        net.tick(); // advance into visible range
        assert_eq!(item_count(&net, out1), 1);
        assert_eq!(item_count(&net, out2), 0);

        // Place another item
        net.spawn_item_on_entity(in1, ItemId::Point);
        advance_to_output(&mut net, in1);

        // Tick 2: should go to second output (out2)
        pool.tick(&mut net);
        net.tick();
        assert_eq!(item_count(&net, out2), 1);
    }

    #[test]
    fn balancer_pairs_inputs_to_outputs() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let (mut pool, in1, in2, out1, out2, _) = setup_balancer(&mut world, &mut net);

        // Place items on both inputs
        net.spawn_item_on_entity(in1, ItemId::NullSet);
        net.spawn_item_on_entity(in2, ItemId::Point);
        advance_to_output(&mut net, in1);

        // One tick should transfer both: in1→out1, in2→out2
        pool.tick(&mut net);
        assert_eq!(item_count(&net, in1), 0);
        assert_eq!(item_count(&net, in2), 0);
        net.tick(); // advance into visible range
        assert_eq!(item_count(&net, out1), 1);
        assert_eq!(item_count(&net, out2), 1);
    }

    #[test]
    fn backpressure_when_output_full() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let (mut pool, in1, out1, out2, _) = setup_splitter(&mut world, &mut net);

        // Fill both output belts: push items and advance them inward so they
        // compress and leave no room. We need to fill the line so that
        // can_accept_at_input returns false.
        // A single-segment belt has length 256, MIN_ITEM_GAP 64 → max ~4 items.
        // Spawn items and compress them.
        for _ in 0..4 {
            net.spawn_item_on_entity(out1, ItemId::NullSet);
            net.spawn_item_on_entity(out2, ItemId::NullSet);
        }
        // Push one more item to each output's input end to fully fill it
        net.push_to_entity_input(out1, ItemId::NullSet);
        net.push_to_entity_input(out2, ItemId::NullSet);

        // Place item on input
        net.spawn_item_on_entity(in1, ItemId::Point);
        advance_to_output(&mut net, in1);

        assert_eq!(item_count(&net, in1), 1, "item should be at output end of input belt");

        // Splitter tick — outputs are full, item should stay on input
        pool.tick(&mut net);
        assert_eq!(item_count(&net, in1), 1, "item should remain when outputs are full");
    }

    #[test]
    fn throughput_limited_to_one_per_tick() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let (mut pool, in1, _in2, out1, _) = setup_merger(&mut world, &mut net);

        // Place items on input belt at positions that will both reach pos=0
        // (only the front item can be at pos=0; the second compresses behind it)
        net.spawn_item_on_entity(in1, ItemId::NullSet);
        advance_to_output(&mut net, in1);

        // Now add second item behind
        net.spawn_item_on_entity(in1, ItemId::Point);

        // One splitter tick should transfer exactly 1 item
        pool.tick(&mut net);
        net.tick(); // advance into visible range
        assert_eq!(item_count(&net, out1), 1, "only 1 item per tick");
        // The second item is still on in1 (not at pos=0 yet, so not transferable)
    }

    #[test]
    fn splitter_skips_full_output_tries_next() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let (mut pool, in1, out1, out2, _) = setup_splitter(&mut world, &mut net);

        // Fill out1 so it can't accept
        for _ in 0..4 {
            net.spawn_item_on_entity(out1, ItemId::NullSet);
        }
        net.push_to_entity_input(out1, ItemId::NullSet);

        // Place item on input
        net.spawn_item_on_entity(in1, ItemId::Point);
        advance_to_output(&mut net, in1);

        // Splitter tick: out1 is full, should send to out2
        pool.tick(&mut net);
        assert_eq!(item_count(&net, in1), 0, "item should have transferred");
        net.tick();
        assert_eq!(item_count(&net, out2), 1, "should go to the non-full output");
    }

    #[test]
    fn balancer_overflow_when_primary_output_full() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let (mut pool, in1, in2, out1, out2, _) = setup_balancer(&mut world, &mut net);

        // Fill out1 (primary output for in1)
        for _ in 0..4 {
            net.spawn_item_on_entity(out1, ItemId::NullSet);
        }
        net.push_to_entity_input(out1, ItemId::NullSet);

        // Place item on in1
        net.spawn_item_on_entity(in1, ItemId::Point);
        advance_to_output(&mut net, in1);

        // in1's primary output (out1) is full, should overflow to out2
        pool.tick(&mut net);
        assert_eq!(item_count(&net, in1), 0);
        net.tick();
        assert_eq!(item_count(&net, out2), 1, "should overflow to non-full output");
        let _ = in2;
    }

    #[test]
    fn inactive_splitter_does_nothing() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];
        let splitter_entity = world.place(addr, (5, 5), ItemId::Splitter, Direction::North).unwrap();

        // Input belt only, no output
        let in1 = place_belt(&mut world, &mut net, addr, 4, 5, Direction::East);
        net.connect_belt_to_splitter(in1, splitter_entity);

        let mut pool = SplitterPool::new();
        pool.add(splitter_entity);
        pool.add_input(splitter_entity, in1);
        pool.detect_mode(splitter_entity);
        assert_eq!(pool.get(splitter_entity).unwrap().mode, SplitterMode::Inactive);

        net.spawn_item_on_entity(in1, ItemId::NullSet);
        advance_to_output(&mut net, in1);

        // Tick should do nothing
        pool.tick(&mut net);
        assert_eq!(item_count(&net, in1), 1, "item should remain — splitter is inactive");
    }
}
