use std::collections::HashMap;
use super::items::ItemId;

#[derive(Clone, Debug)]
pub struct Inventory {
    items: HashMap<ItemId, u32>,
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }

    pub fn starting_inventory() -> Self {
        let mut inv = Self::new();
        inv.add(ItemId::Belt, 1);
        inv.add(ItemId::Quadrupole, 1);
        inv
    }

    pub fn add(&mut self, item: ItemId, count: u32) {
        *self.items.entry(item).or_insert(0) += count;
    }

    pub fn remove(&mut self, item: ItemId, count: u32) -> bool {
        let current = self.items.get(&item).copied().unwrap_or(0);
        if current < count {
            return false;
        }
        if current == count {
            self.items.remove(&item);
        } else {
            self.items.insert(item, current - count);
        }
        true
    }

    pub fn count(&self, item: ItemId) -> u32 {
        self.items.get(&item).copied().unwrap_or(0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ItemId, &u32)> {
        self.items.iter()
    }

    pub fn non_empty_items(&self) -> Vec<(ItemId, u32)> {
        let mut items: Vec<(ItemId, u32)> = self.items
            .iter()
            .filter(|(_, &count)| count > 0)
            .map(|(&id, &count)| (id, count))
            .collect();
        items.sort_by_key(|(id, _)| {
            ItemId::all().iter().position(|i| i == id).unwrap_or(usize::MAX)
        });
        items
    }
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inventory_add_remove() {
        let mut inv = Inventory::new();
        inv.add(ItemId::Point, 10);
        assert_eq!(inv.count(ItemId::Point), 10);
        assert!(inv.remove(ItemId::Point, 5));
        assert_eq!(inv.count(ItemId::Point), 5);
        assert!(inv.remove(ItemId::Point, 5));
        assert_eq!(inv.count(ItemId::Point), 0);
    }

    #[test]
    fn test_inventory_remove_insufficient() {
        let mut inv = Inventory::new();
        inv.add(ItemId::Point, 3);
        assert!(!inv.remove(ItemId::Point, 5));
        assert_eq!(inv.count(ItemId::Point), 3); // unchanged
    }

    #[test]
    fn test_starting_inventory_contents() {
        let inv = Inventory::starting_inventory();
        assert_eq!(inv.count(ItemId::Belt), 1);
        assert_eq!(inv.count(ItemId::Quadrupole), 1);
        assert_eq!(inv.count(ItemId::Point), 0);
    }

    #[test]
    fn test_non_empty_items() {
        let mut inv = Inventory::new();
        inv.add(ItemId::Belt, 5);
        inv.add(ItemId::Point, 10);
        let items = inv.non_empty_items();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut inv = Inventory::new();
        assert!(!inv.remove(ItemId::Cube, 1));
    }
}
