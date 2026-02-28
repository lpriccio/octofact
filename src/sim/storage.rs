use std::collections::HashMap;

use crate::game::items::ItemId;
use crate::game::world::EntityId;
use crate::sim::machine::ItemStack;

/// Maximum number of storage slots per building.
pub const STORAGE_SLOTS: usize = 20;
/// Maximum items per stack in a storage slot.
pub const STORAGE_STACK_SIZE: u16 = 50;

/// Per-storage state.
#[derive(Clone, Debug)]
pub struct StorageState {
    pub entity: EntityId,
    pub slots: [ItemStack; STORAGE_SLOTS],
}

/// Pool of all placed storage buildings. Dense storage indexed by EntityId.
pub struct StoragePool {
    storages: Vec<StorageState>,
    entity_to_idx: HashMap<EntityId, usize>,
}

impl StoragePool {
    pub fn new() -> Self {
        Self {
            storages: Vec::new(),
            entity_to_idx: HashMap::new(),
        }
    }

    /// Register a newly placed storage building.
    pub fn add(&mut self, entity: EntityId) {
        let idx = self.storages.len();
        self.storages.push(StorageState {
            entity,
            slots: [ItemStack { item: ItemId::NullSet, count: 0 }; STORAGE_SLOTS],
        });
        self.entity_to_idx.insert(entity, idx);
    }

    /// Remove a storage building by EntityId. Swap-removes with the last element.
    pub fn remove(&mut self, entity: EntityId) -> bool {
        let Some(idx) = self.entity_to_idx.remove(&entity) else {
            return false;
        };
        let last = self.storages.len() - 1;

        if idx != last {
            self.storages.swap(idx, last);
            let swapped_entity = self.storages[idx].entity;
            self.entity_to_idx.insert(swapped_entity, idx);
        }

        self.storages.pop();
        true
    }

    /// Get a reference to the storage state for an entity.
    pub fn get(&self, entity: EntityId) -> Option<&StorageState> {
        self.entity_to_idx.get(&entity)
            .map(|&i| &self.storages[i])
    }

    /// Get a mutable reference to the storage state for an entity.
    pub fn get_mut(&mut self, entity: EntityId) -> Option<&mut StorageState> {
        let &i = self.entity_to_idx.get(&entity)?;
        Some(&mut self.storages[i])
    }

    /// Try to store an item in the storage building.
    /// Finds the first slot with the matching item that has room, or the first empty slot.
    /// Returns true if the item was accepted, false if all 20 slots are full.
    pub fn accept_input(&mut self, entity: EntityId, item: ItemId, count: u16) -> bool {
        let state = match self.get_mut(entity) {
            Some(s) => s,
            None => return false,
        };

        // First pass: try to stack into an existing slot with the same item
        for slot in state.slots.iter_mut() {
            if slot.item == item && slot.count > 0 && slot.count + count <= STORAGE_STACK_SIZE {
                slot.count += count;
                return true;
            }
        }

        // Second pass: try to place into an empty slot
        for slot in state.slots.iter_mut() {
            if slot.count == 0 {
                slot.item = item;
                slot.count = count;
                return true;
            }
        }

        false // all slots full or occupied by different items at max stack
    }

    /// Try to take one item from the storage for output.
    /// Scans slots sequentially and takes from the first non-empty stack.
    /// Returns the ItemId taken, or None if the storage is empty.
    pub fn provide_output(&mut self, entity: EntityId) -> Option<ItemId> {
        let state = self.get_mut(entity)?;

        for slot in state.slots.iter_mut() {
            if slot.count > 0 {
                let item = slot.item;
                slot.count -= 1;
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
        let mut pool = StoragePool::new();
        let (_, e1) = make_entity();
        pool.add(e1);

        let state = pool.get(e1).unwrap();
        assert_eq!(state.entity, e1);
        // All slots should be empty
        for slot in &state.slots {
            assert_eq!(slot.count, 0);
        }
    }

    #[test]
    fn remove_returns_true() {
        let mut pool = StoragePool::new();
        let (_, e1) = make_entity();
        pool.add(e1);
        assert!(pool.remove(e1));
        assert!(pool.get(e1).is_none());
    }

    #[test]
    fn remove_nonexistent() {
        let mut pool = StoragePool::new();
        let (_, e1) = make_entity();
        assert!(!pool.remove(e1));
    }

    #[test]
    fn remove_swap() {
        let mut pool = StoragePool::new();
        let mut sm: SlotMap<EntityId, ()> = SlotMap::with_key();
        let e1 = sm.insert(());
        let e2 = sm.insert(());
        let e3 = sm.insert(());

        pool.add(e1);
        pool.add(e2);
        pool.add(e3);

        // Remove middle — e3 should swap into index 1
        assert!(pool.remove(e2));
        assert!(pool.get(e1).is_some());
        assert!(pool.get(e2).is_none());
        assert!(pool.get(e3).is_some());
    }

    #[test]
    fn slot_constants() {
        assert_eq!(STORAGE_SLOTS, 20);
        assert_eq!(STORAGE_STACK_SIZE, 50);
    }

    #[test]
    fn accept_input_into_empty_slot() {
        let mut pool = StoragePool::new();
        let (_, e1) = make_entity();
        pool.add(e1);

        assert!(pool.accept_input(e1, ItemId::Point, 1));
        let state = pool.get(e1).unwrap();
        assert_eq!(state.slots[0].item, ItemId::Point);
        assert_eq!(state.slots[0].count, 1);
    }

    #[test]
    fn accept_input_stacks_same_item() {
        let mut pool = StoragePool::new();
        let (_, e1) = make_entity();
        pool.add(e1);

        assert!(pool.accept_input(e1, ItemId::Point, 1));
        assert!(pool.accept_input(e1, ItemId::Point, 1));
        let state = pool.get(e1).unwrap();
        assert_eq!(state.slots[0].item, ItemId::Point);
        assert_eq!(state.slots[0].count, 2);
    }

    #[test]
    fn accept_input_mixed_items_separate_slots() {
        let mut pool = StoragePool::new();
        let (_, e1) = make_entity();
        pool.add(e1);

        assert!(pool.accept_input(e1, ItemId::Point, 1));
        assert!(pool.accept_input(e1, ItemId::LineSegment, 1));
        assert!(pool.accept_input(e1, ItemId::NullSet, 1));

        let state = pool.get(e1).unwrap();
        assert_eq!(state.slots[0].item, ItemId::Point);
        assert_eq!(state.slots[0].count, 1);
        assert_eq!(state.slots[1].item, ItemId::LineSegment);
        assert_eq!(state.slots[1].count, 1);
        assert_eq!(state.slots[2].item, ItemId::NullSet);
        assert_eq!(state.slots[2].count, 1);
    }

    #[test]
    fn accept_input_rejects_when_all_slots_full() {
        let mut pool = StoragePool::new();
        let (_, e1) = make_entity();
        pool.add(e1);

        // Fill all 20 slots with max stacks of different items
        // We only have a finite set of items, so fill with one item type at max stack
        for _ in 0..STORAGE_SLOTS {
            // Each slot gets a different item to prevent stacking
            // But we don't have 20 unique items easily, so fill one item to max
            // then it should go to a new slot
        }
        // Simpler: fill all slots with Point at max stack size
        {
            let state = pool.get_mut(e1).unwrap();
            for slot in state.slots.iter_mut() {
                slot.item = ItemId::Point;
                slot.count = STORAGE_STACK_SIZE;
            }
        }

        // Now trying to add more Points should fail (all at max stack)
        assert!(!pool.accept_input(e1, ItemId::Point, 1));
        // And a different item should also fail (no empty slots)
        assert!(!pool.accept_input(e1, ItemId::LineSegment, 1));
    }

    #[test]
    fn provide_output_from_nonempty() {
        let mut pool = StoragePool::new();
        let (_, e1) = make_entity();
        pool.add(e1);

        pool.accept_input(e1, ItemId::Point, 3);
        let item = pool.provide_output(e1);
        assert_eq!(item, Some(ItemId::Point));
        assert_eq!(pool.get(e1).unwrap().slots[0].count, 2);
    }

    #[test]
    fn provide_output_returns_none_when_empty() {
        let mut pool = StoragePool::new();
        let (_, e1) = make_entity();
        pool.add(e1);

        assert_eq!(pool.provide_output(e1), None);
    }

    #[test]
    fn provide_output_drains_first_nonempty_slot() {
        let mut pool = StoragePool::new();
        let (_, e1) = make_entity();
        pool.add(e1);

        pool.accept_input(e1, ItemId::Point, 1);
        pool.accept_input(e1, ItemId::LineSegment, 1);

        // Should drain from slot 0 (Point) first
        assert_eq!(pool.provide_output(e1), Some(ItemId::Point));
        // Slot 0 is now empty, next should be slot 1 (LineSegment)
        assert_eq!(pool.provide_output(e1), Some(ItemId::LineSegment));
        // Now both empty
        assert_eq!(pool.provide_output(e1), None);
    }
}
