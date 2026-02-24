use crate::game::inventory::Inventory;
use crate::game::items::ItemId;
use crate::game::world::Direction;
use super::icons::IconAtlas;

/// Items that can be placed on the grid.
const PLACEABLE_ITEMS: &[ItemId] = &[ItemId::Belt, ItemId::Quadrupole];

#[derive(Clone, Debug)]
pub struct PlacementMode {
    pub item: ItemId,
    pub direction: Direction,
}

pub fn placement_panel(
    ctx: &egui::Context,
    open: &mut bool,
    inventory: &Inventory,
    icons: &IconAtlas,
    current_mode: &mut Option<PlacementMode>,
) {
    if !*open {
        return;
    }

    egui::Window::new("Placement")
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(8.0, 8.0))
        .collapsible(false)
        .resizable(false)
        .default_width(180.0)
        .show(ctx, |ui| {
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
            for &item_id in PLACEABLE_ITEMS {
                let count = inventory.count(item_id);
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
                    if ui.add(egui::Label::new(label).sense(egui::Sense::click())).clicked() && count > 0 {
                        *current_mode = Some(PlacementMode {
                            item: item_id,
                            direction: current_mode
                                .as_ref()
                                .map(|m| m.direction)
                                .unwrap_or(Direction::North),
                        });
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("x{count}"))
                                .color(if count > 0 {
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
