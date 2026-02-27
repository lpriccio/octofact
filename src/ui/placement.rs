use crate::game::inventory::Inventory;
use crate::game::items::ItemId;
use crate::game::world::{Direction, StructureKind};
use super::icons::IconAtlas;

/// Items shown in the placement panel by default (when not in free-placement mode).
const PLACEABLE_ITEMS: &[ItemId] = &[ItemId::Belt, ItemId::Splitter, ItemId::Quadrupole];

#[derive(Clone, Debug)]
pub struct PlacementMode {
    pub item: ItemId,
    pub direction: Direction,
}

/// All items that have a StructureKind (i.e. can be placed on the grid).
fn all_placeable_items() -> Vec<ItemId> {
    ItemId::all()
        .iter()
        .copied()
        .filter(|id| StructureKind::from_item(*id).is_some())
        .collect()
}

pub fn placement_panel(
    ctx: &egui::Context,
    open: &mut bool,
    inventory: &Inventory,
    icons: &IconAtlas,
    current_mode: &mut Option<PlacementMode>,
    free_placement: bool,
) {
    if !*open {
        return;
    }

    let items: Vec<ItemId> = if free_placement {
        all_placeable_items()
    } else {
        PLACEABLE_ITEMS.to_vec()
    };

    egui::Window::new("Placement")
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(8.0, 8.0))
        .collapsible(false)
        .resizable(false)
        .default_width(180.0)
        .show(ctx, |ui| {
            if free_placement {
                ui.label(
                    egui::RichText::new("FREE PLACEMENT")
                        .color(egui::Color32::from_rgb(255, 200, 50))
                        .strong()
                        .size(11.0),
                );
                ui.separator();
            }

            // Current selection header
            if let Some(mode) = current_mode.as_ref() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Placing: {} {}",
                            mode.item.display_name(),
                            mode.direction.arrow_char(),
                        ))
                        .strong(),
                    );
                });
                ui.separator();
            }

            // Placeable items list
            for &item_id in &items {
                let count = inventory.count(item_id);
                let can_place = free_placement || count > 0;
                let selected = current_mode.as_ref().map(|m| m.item == item_id).unwrap_or(false);

                let response = ui.horizontal(|ui| {
                    if let Some(tex) = icons.get(item_id) {
                        ui.image(egui::load::SizedTexture::new(
                            tex.id(),
                            egui::vec2(20.0, 20.0),
                        ));
                    }

                    let label = if selected {
                        egui::RichText::new(item_id.display_name())
                            .strong()
                            .color(egui::Color32::from_rgb(120, 200, 255))
                    } else {
                        egui::RichText::new(item_id.display_name())
                    };
                    if ui.add(egui::Label::new(label).sense(egui::Sense::click())).clicked() && can_place {
                        *current_mode = Some(PlacementMode {
                            item: item_id,
                            direction: current_mode
                                .as_ref()
                                .map(|m| m.direction)
                                .unwrap_or(Direction::North),
                        });
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let count_text = if free_placement {
                            "\u{221e}".to_string() // infinity symbol
                        } else {
                            format!("x{count}")
                        };
                        ui.label(
                            egui::RichText::new(count_text)
                                .color(if can_place {
                                    egui::Color32::from_gray(200)
                                } else {
                                    egui::Color32::from_gray(80)
                                }),
                        );
                    });
                });

                if response.response.hovered() {
                    response.response.show_tooltip_text(item_id.description());
                }
            }

            ui.separator();

            if current_mode.is_some() && ui.button("Cancel placement").clicked() {
                *current_mode = None;
            }

            ui.label(
                egui::RichText::new("R to rotate | Click to place")
                    .weak()
                    .size(11.0),
            );
        });
}
