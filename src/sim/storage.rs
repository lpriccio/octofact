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
}
