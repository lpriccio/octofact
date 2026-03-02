use crate::game::items::ItemId;
use crate::game::recipes::RecipeIndex;
use super::icons::IconAtlas;

pub fn item_tooltip(
    ui: &mut egui::Ui,
    item: ItemId,
    icons: &IconAtlas,
    recipes: &RecipeIndex,
) {
    ui.horizontal(|ui| {
        if let Some(tex) = icons.get(item) {
            ui.image(egui::load::SizedTexture::new(
                tex.id(),
                egui::vec2(32.0, 32.0),
            ));
        }
        ui.vertical(|ui| {
            ui.strong(item.display_name());
            ui.label(format!("Tier {} | {}", item.tier(), item.category().display_name()));
        });
    });

    ui.separator();
    ui.label(item.description());

    let item_recipes = recipes.recipes_for(item);
    if !item_recipes.is_empty() {
        ui.separator();
        ui.label("Recipes:");
        for recipe in item_recipes {
            let inputs: Vec<String> = recipe.inputs
                .iter()
                .map(|(id, count)| {
                    if *count > 1 {
                        format!("{}x {}", count, id.display_name())
                    } else {
                        id.display_name().to_string()
                    }
                })
                .collect();
            ui.label(format!(
                "  {} -> {}",
                recipe.machine.display_name(),
                inputs.join(" + ")
            ));
        }
    }
}
