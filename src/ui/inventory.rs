use crate::game::inventory::Inventory;
use crate::game::items::ItemCategory;
use crate::game::recipes::RecipeIndex;
use super::icons::IconAtlas;
use super::tooltip::item_tooltip;

pub fn inventory_window(
    ctx: &egui::Context,
    open: &mut bool,
    inventory: &Inventory,
    icons: &IconAtlas,
    recipes: &RecipeIndex,
) {
    if !*open {
        return;
    }

    egui::Window::new("Inventory")
        .collapsible(true)
        .resizable(true)
        .default_width(350.0)
        .default_height(450.0)
        .show(ctx, |ui| {
            let items = inventory.non_empty_items();

            if items.is_empty() {
                ui.label("Inventory is empty.");
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                for &category in ItemCategory::all() {
                    let category_items: Vec<_> = items
                        .iter()
                        .filter(|(id, _)| id.category() == category)
                        .collect();

                    if category_items.is_empty() {
                        continue;
                    }

                    ui.collapsing(category.display_name(), |ui| {
                        for &&(item_id, count) in &category_items {
                            let response = ui.horizontal(|ui| {
                                // Icon
                                if let Some(tex) = icons.get(item_id) {
                                    ui.image(egui::load::SizedTexture::new(
                                        tex.id(),
                                        egui::vec2(24.0, 24.0),
                                    ));
                                }

                                ui.label(item_id.display_name());

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(format!("x{count}"));
                                    },
                                );
                            });

                            // Tooltip on hover
                            if response.response.hovered() {
                                response.response.show_tooltip_ui(|ui| {
                                    item_tooltip(ui, item_id, icons, recipes);
                                });
                            }
                        }
                    });
                }
            });
        });
}
