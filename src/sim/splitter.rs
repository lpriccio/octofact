use std::collections::HashMap;

use crate::game::world::EntityId;

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

    /// Remove a belt entity from this splitter's inputs and outputs.
    pub fn disconnect_belt(&mut self, splitter: EntityId, belt: EntityId) {
        if let Some(s) = self.get_mut(splitter) {
            s.inputs.retain(|&e| e != belt);
            s.outputs.retain(|&e| e != belt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;

    fn make_entity() -> (SlotMap<EntityId, ()>, EntityId) {
        let mut sm = SlotMap::with_key();
        let id = sm.insert(());
        (sm, id)
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
}
