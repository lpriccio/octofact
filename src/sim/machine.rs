#![allow(dead_code)] // Types defined here are wired in by later milestones.

use std::collections::HashMap;

use crate::game::items::{ItemId, MachineType};
use crate::game::world::EntityId;

/// Default crafting duration in ticks (60 UPS = 2 seconds).
pub const DEFAULT_CRAFT_TICKS: u16 = 120;

/// An item type + count, used for machine input/output slots.
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: u16,
}

/// Machine processing state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineState {
    /// No recipe set, or recipe complete and waiting for new inputs.
    Idle,
    /// Actively crafting (decrementing recipe_ticks each tick).
    Working,
    /// Craft finished but output slots are full.
    OutputFull,
    /// No inputs available for the selected recipe.
    NoInput,
    /// Insufficient power (power satisfaction too low).
    NoPower,
}

/// Maximum number of input/output slots per machine.
pub const MAX_SLOTS: usize = 4;

/// Hot data — touched every simulation tick. Kept contiguous for cache performance.
pub struct MachineHotData {
    /// Crafting progress [0.0 .. 1.0].
    pub progress: Vec<f32>,
    /// Ticks remaining on current craft. 0 when idle.
    pub recipe_ticks: Vec<u16>,
    /// Total ticks for the active recipe (for computing progress ratio).
    pub recipe_total_ticks: Vec<u16>,
    /// Power satisfaction [0.0 .. 1.0]. Set by power network each tick.
    pub power_draw: Vec<f32>,
    /// Current state.
    pub state: Vec<MachineState>,
}

/// Cold data — touched on interaction (UI, inserter delivery, recipe selection).
pub struct MachineColdData {
    /// Which entity in the world this machine corresponds to.
    pub entity_id: Vec<EntityId>,
    /// Machine variant (Composer, Inverter, etc.).
    pub machine_type: Vec<MachineType>,
    /// Selected recipe (index into RecipeIndex::all). None if no recipe set.
    pub recipe: Vec<Option<usize>>,
    /// Input item slots.
    pub input_slots: Vec<[ItemStack; MAX_SLOTS]>,
    /// Output item slots.
    pub output_slots: Vec<[ItemStack; MAX_SLOTS]>,
}

/// SoA machine pool. Hot and cold vecs are indexed by the same dense index.
/// Use `entity_to_idx` for EntityId -> index lookup.
pub struct MachinePool {
    pub hot: MachineHotData,
    pub cold: MachineColdData,
    /// Number of active machines.
    pub count: usize,
    /// EntityId -> dense index mapping.
    entity_to_idx: HashMap<EntityId, usize>,
}

impl MachinePool {
    pub fn new() -> Self {
        Self {
            hot: MachineHotData {
                progress: Vec::new(),
                recipe_ticks: Vec::new(),
                recipe_total_ticks: Vec::new(),
                power_draw: Vec::new(),
                state: Vec::new(),
            },
            cold: MachineColdData {
                entity_id: Vec::new(),
                machine_type: Vec::new(),
                recipe: Vec::new(),
                input_slots: Vec::new(),
                output_slots: Vec::new(),
            },
            count: 0,
            entity_to_idx: HashMap::new(),
        }
    }

    /// Register a newly placed machine. Returns the dense index.
    pub fn add(&mut self, entity: EntityId, machine_type: MachineType) -> usize {
        let idx = self.count;

        // Hot data
        self.hot.progress.push(0.0);
        self.hot.recipe_ticks.push(0);
        self.hot.recipe_total_ticks.push(0);
        self.hot.power_draw.push(1.0); // full power until power network says otherwise
        self.hot.state.push(MachineState::Idle);

        // Cold data
        self.cold.entity_id.push(entity);
        self.cold.machine_type.push(machine_type);
        self.cold.recipe.push(None);
        self.cold.input_slots.push([ItemStack::default(); MAX_SLOTS]);
        self.cold.output_slots.push([ItemStack::default(); MAX_SLOTS]);

        self.entity_to_idx.insert(entity, idx);
        self.count += 1;
        idx
    }

    /// Remove a machine by EntityId. Swap-removes with the last element.
    pub fn remove(&mut self, entity: EntityId) -> bool {
        let Some(idx) = self.entity_to_idx.remove(&entity) else {
            return false;
        };
        let last = self.count - 1;

        if idx != last {
            // Swap hot data
            self.hot.progress.swap(idx, last);
            self.hot.recipe_ticks.swap(idx, last);
            self.hot.recipe_total_ticks.swap(idx, last);
            self.hot.power_draw.swap(idx, last);
            self.hot.state.swap(idx, last);

            // Swap cold data
            self.cold.entity_id.swap(idx, last);
            self.cold.machine_type.swap(idx, last);
            self.cold.recipe.swap(idx, last);
            self.cold.input_slots.swap(idx, last);
            self.cold.output_slots.swap(idx, last);

            // Update the swapped entity's index
            let swapped_entity = self.cold.entity_id[idx];
            self.entity_to_idx.insert(swapped_entity, idx);
        }

        // Pop the last element
        self.hot.progress.pop();
        self.hot.recipe_ticks.pop();
        self.hot.recipe_total_ticks.pop();
        self.hot.power_draw.pop();
        self.hot.state.pop();
        self.cold.entity_id.pop();
        self.cold.machine_type.pop();
        self.cold.recipe.pop();
        self.cold.input_slots.pop();
        self.cold.output_slots.pop();

        self.count -= 1;
        true
    }

    /// Look up the dense index for an EntityId.
    pub fn index_of(&self, entity: EntityId) -> Option<usize> {
        self.entity_to_idx.get(&entity).copied()
    }

    /// Get the MachineState for an entity.
    pub fn state(&self, entity: EntityId) -> Option<MachineState> {
        self.index_of(entity).map(|i| self.hot.state[i])
    }

    /// Get the machine type for an entity.
    pub fn machine_type(&self, entity: EntityId) -> Option<MachineType> {
        self.index_of(entity).map(|i| self.cold.machine_type[i])
    }

    /// Get the selected recipe index for an entity.
    pub fn recipe(&self, entity: EntityId) -> Option<Option<usize>> {
        self.index_of(entity).map(|i| self.cold.recipe[i])
    }

    /// Set the recipe for a machine. Resets crafting progress.
    pub fn set_recipe(&mut self, entity: EntityId, recipe_idx: Option<usize>) {
        if let Some(i) = self.index_of(entity) {
            self.cold.recipe[i] = recipe_idx;
            self.hot.progress[i] = 0.0;
            self.hot.recipe_ticks[i] = 0;
            self.hot.recipe_total_ticks[i] = 0;
            self.hot.state[i] = MachineState::Idle;
        }
    }

    /// Get a reference to the input slots for an entity.
    pub fn input_slots(&self, entity: EntityId) -> Option<&[ItemStack; MAX_SLOTS]> {
        self.index_of(entity).map(|i| &self.cold.input_slots[i])
    }

    /// Get a reference to the output slots for an entity.
    pub fn output_slots(&self, entity: EntityId) -> Option<&[ItemStack; MAX_SLOTS]> {
        self.index_of(entity).map(|i| &self.cold.output_slots[i])
    }

    /// Get crafting progress [0.0 .. 1.0] for an entity.
    pub fn progress(&self, entity: EntityId) -> Option<f32> {
        self.index_of(entity).map(|i| self.hot.progress[i])
    }

    /// Try to insert an item into a machine's input slots. Returns true if accepted.
    pub fn insert_input(&mut self, entity: EntityId, item: ItemId, count: u16) -> bool {
        let Some(i) = self.index_of(entity) else {
            return false;
        };
        let slots = &mut self.cold.input_slots[i];

        // Try to stack into an existing slot with the same item
        for slot in slots.iter_mut() {
            if slot.item == item && slot.count > 0 {
                slot.count += count;
                return true;
            }
        }
        // Try to place into an empty slot
        for slot in slots.iter_mut() {
            if slot.count == 0 {
                slot.item = item;
                slot.count = count;
                return true;
            }
        }
        false // all slots occupied by different items
    }

    /// Try to take an item from a machine's output slots. Returns the item taken, if any.
    pub fn take_output(&mut self, entity: EntityId) -> Option<ItemId> {
        let i = self.index_of(entity)?;
        let slots = &mut self.cold.output_slots[i];

        for slot in slots.iter_mut() {
            if slot.count > 0 {
                let item = slot.item;
                slot.count -= 1;
                // Wake machine if it was blocked on full output
                if self.hot.state[i] == MachineState::OutputFull {
                    self.hot.state[i] = MachineState::Idle;
                }
                return Some(item);
            }
        }
        None
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
        let mut pool = MachinePool::new();
        let (mut sm, e1) = make_entity();
        let e2 = sm.insert(());

        let idx1 = pool.add(e1, MachineType::Composer);
        let idx2 = pool.add(e2, MachineType::Inverter);

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(pool.count, 2);
        assert_eq!(pool.state(e1), Some(MachineState::Idle));
        assert_eq!(pool.machine_type(e2), Some(MachineType::Inverter));
    }

    #[test]
    fn remove_swap() {
        let mut pool = MachinePool::new();
        let mut sm: SlotMap<EntityId, ()> = SlotMap::with_key();
        let e1 = sm.insert(());
        let e2 = sm.insert(());
        let e3 = sm.insert(());

        pool.add(e1, MachineType::Composer);
        pool.add(e2, MachineType::Inverter);
        pool.add(e3, MachineType::Embedder);

        // Remove middle element — e3 should swap into index 1
        assert!(pool.remove(e2));
        assert_eq!(pool.count, 2);
        assert_eq!(pool.index_of(e2), None);
        assert_eq!(pool.index_of(e1), Some(0));
        assert_eq!(pool.index_of(e3), Some(1));
        assert_eq!(pool.machine_type(e3), Some(MachineType::Embedder));
    }

    #[test]
    fn remove_last_no_swap() {
        let mut pool = MachinePool::new();
        let (mut sm, e1) = make_entity();
        let e2 = sm.insert(());

        pool.add(e1, MachineType::Composer);
        pool.add(e2, MachineType::Inverter);

        assert!(pool.remove(e2));
        assert_eq!(pool.count, 1);
        assert_eq!(pool.index_of(e1), Some(0));
        assert_eq!(pool.machine_type(e1), Some(MachineType::Composer));
    }

    #[test]
    fn remove_nonexistent() {
        let mut pool = MachinePool::new();
        let (_, e1) = make_entity();
        assert!(!pool.remove(e1));
    }

    #[test]
    fn set_recipe_resets_state() {
        let mut pool = MachinePool::new();
        let (_, e1) = make_entity();
        pool.add(e1, MachineType::Composer);

        // Simulate being in Working state
        let i = pool.index_of(e1).unwrap();
        pool.hot.state[i] = MachineState::Working;
        pool.hot.progress[i] = 0.5;
        pool.hot.recipe_ticks[i] = 60;

        pool.set_recipe(e1, Some(0));
        assert_eq!(pool.hot.state[i], MachineState::Idle);
        assert_eq!(pool.hot.progress[i], 0.0);
        assert_eq!(pool.hot.recipe_ticks[i], 0);
        assert_eq!(pool.cold.recipe[i], Some(0));
    }

    #[test]
    fn input_output_slots() {
        let mut pool = MachinePool::new();
        let (_, e1) = make_entity();
        pool.add(e1, MachineType::Composer);

        // Insert items into input
        assert!(pool.insert_input(e1, ItemId::Point, 2));
        assert!(pool.insert_input(e1, ItemId::Point, 3)); // stacks
        assert!(pool.insert_input(e1, ItemId::NullSet, 1)); // new slot

        let slots = pool.input_slots(e1).unwrap();
        assert_eq!(slots[0].item, ItemId::Point);
        assert_eq!(slots[0].count, 5);
        assert_eq!(slots[1].item, ItemId::NullSet);
        assert_eq!(slots[1].count, 1);

        // Fill remaining slots
        assert!(pool.insert_input(e1, ItemId::Preimage, 1));
        assert!(pool.insert_input(e1, ItemId::Wavelet, 1));
        // All 4 slots full with different items — should reject new item type
        assert!(!pool.insert_input(e1, ItemId::LineSegment, 1));
        // But can still stack existing items
        assert!(pool.insert_input(e1, ItemId::Point, 1));
    }

    #[test]
    fn take_output() {
        let mut pool = MachinePool::new();
        let (_, e1) = make_entity();
        pool.add(e1, MachineType::Composer);

        // Put items in output slots directly
        let i = pool.index_of(e1).unwrap();
        pool.cold.output_slots[i][0] = ItemStack {
            item: ItemId::LineSegment,
            count: 2,
        };
        pool.hot.state[i] = MachineState::OutputFull;

        // Take one
        assert_eq!(pool.take_output(e1), Some(ItemId::LineSegment));
        assert_eq!(pool.cold.output_slots[i][0].count, 1);
        // Machine should wake from OutputFull
        assert_eq!(pool.hot.state[i], MachineState::Idle);

        // Take another
        assert_eq!(pool.take_output(e1), Some(ItemId::LineSegment));
        assert_eq!(pool.cold.output_slots[i][0].count, 0);

        // Nothing left
        assert_eq!(pool.take_output(e1), None);
    }
}
