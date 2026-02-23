use std::collections::HashMap;
use super::items::{all_recipes, ItemId, MachineType, Recipe};
use super::inventory::Inventory;

pub struct RecipeIndex {
    pub all: Vec<Recipe>,
    by_output: HashMap<ItemId, Vec<usize>>,
    by_machine: HashMap<MachineType, Vec<usize>>,
}

impl RecipeIndex {
    pub fn new() -> Self {
        let all = all_recipes();
        let mut by_output: HashMap<ItemId, Vec<usize>> = HashMap::new();
        let mut by_machine: HashMap<MachineType, Vec<usize>> = HashMap::new();

        for (i, recipe) in all.iter().enumerate() {
            by_output.entry(recipe.output).or_default().push(i);
            by_machine.entry(recipe.machine).or_default().push(i);
        }

        Self { all, by_output, by_machine }
    }

    pub fn recipes_for(&self, output: ItemId) -> Vec<&Recipe> {
        self.by_output
            .get(&output)
            .map(|indices| indices.iter().map(|&i| &self.all[i]).collect())
            .unwrap_or_default()
    }

    pub fn recipes_using(&self, machine: MachineType) -> Vec<&Recipe> {
        self.by_machine
            .get(&machine)
            .map(|indices| indices.iter().map(|&i| &self.all[i]).collect())
            .unwrap_or_default()
    }

    pub fn can_craft(&self, recipe: &Recipe, inventory: &Inventory) -> bool {
        recipe.inputs.iter().all(|(item, count)| inventory.count(*item) >= *count)
    }
}

impl Default for RecipeIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recipe_index_lookup_by_output() {
        let index = RecipeIndex::new();
        let recipes = index.recipes_for(ItemId::LineSegment);
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].output, ItemId::LineSegment);
    }

    #[test]
    fn test_recipe_index_lookup_by_machine() {
        let index = RecipeIndex::new();
        let composer_recipes = index.recipes_using(MachineType::Composer);
        assert!(composer_recipes.len() > 5, "Composer should have many recipes");
        for r in &composer_recipes {
            assert_eq!(r.machine, MachineType::Composer);
        }
    }

    #[test]
    fn test_can_craft_with_sufficient_resources() {
        let index = RecipeIndex::new();
        let mut inv = Inventory::new();
        inv.add(ItemId::Point, 10);
        let recipes = index.recipes_for(ItemId::LineSegment);
        assert!(index.can_craft(recipes[0], &inv));
    }

    #[test]
    fn test_cannot_craft_with_insufficient_resources() {
        let index = RecipeIndex::new();
        let inv = Inventory::new();
        let recipes = index.recipes_for(ItemId::LineSegment);
        assert!(!index.can_craft(recipes[0], &inv));
    }

    #[test]
    fn test_all_recipes_indexed() {
        let index = RecipeIndex::new();
        assert_eq!(index.all.len(), all_recipes().len());
    }

    #[test]
    fn test_embedder_recipes() {
        let index = RecipeIndex::new();
        let embedder_recipes = index.recipes_using(MachineType::Embedder);
        assert_eq!(embedder_recipes.len(), 3);
    }
}
