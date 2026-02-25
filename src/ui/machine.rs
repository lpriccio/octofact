use crate::game::recipes::RecipeIndex;
use crate::game::world::EntityId;
use crate::sim::machine::{MachinePool, MachineState, MAX_SLOTS};
use super::icons::IconAtlas;

/// Actions the machine panel can produce for the caller to apply.
pub enum MachineAction {
    /// User selected a recipe (or cleared it with None).
    SetRecipe(EntityId, Option<usize>),
    /// User closed the panel.
    Close,
}

/// Draw the machine inspection panel. Returns an action if the user interacted.
pub fn machine_panel(
    ctx: &egui::Context,
    entity: EntityId,
    machine_pool: &MachinePool,
    recipes: &RecipeIndex,
    icons: &IconAtlas,
) -> Option<MachineAction> {
    let idx = machine_pool.index_of(entity)?;
    let machine_type = machine_pool.cold.machine_type[idx];
    let state = machine_pool.hot.state[idx];
    let progress = machine_pool.hot.progress[idx];
    let current_recipe = machine_pool.cold.recipe[idx];
    let input_slots = &machine_pool.cold.input_slots[idx];
    let output_slots = &machine_pool.cold.output_slots[idx];

    let mut action = None;

    let title = machine_type.display_name().to_string();
    let mut open = true;

    egui::Window::new(title)
        .open(&mut open)
        .collapsible(true)
        .resizable(false)
        .default_width(280.0)
        .show(ctx, |ui| {
            // --- Status ---
            ui.horizontal(|ui| {
                ui.label("Status:");
                let (status_text, status_color) = state_display(state);
                ui.colored_label(status_color, status_text);
            });

            // --- Progress bar ---
            if state == MachineState::Working || state == MachineState::OutputFull {
                ui.add(
                    egui::ProgressBar::new(progress)
                        .text(format!("{:.0}%", progress * 100.0))
                        .fill(egui::Color32::from_rgb(102, 77, 179)),
                );
            }

            ui.separator();

            // --- Recipe selector ---
            ui.label("Recipe:");
            let available = recipes.recipes_for_machine(machine_type);
            let current_label = current_recipe
                .and_then(|ri| recipes.all.get(ri))
                .map(recipe_label)
                .unwrap_or_else(|| "None".to_string());

            egui::ComboBox::from_id_salt("recipe_select")
                .selected_text(&current_label)
                .width(240.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current_recipe.is_none(), "None")
                        .clicked()
                    {
                        action = Some(MachineAction::SetRecipe(entity, None));
                    }
                    for (recipe_idx, recipe) in &available {
                        let label = recipe_label(recipe);
                        let selected = current_recipe == Some(*recipe_idx);
                        if ui.selectable_label(selected, &label).clicked() {
                            action = Some(MachineAction::SetRecipe(entity, Some(*recipe_idx)));
                        }
                    }
                });

            ui.separator();

            // --- Input slots ---
            ui.label("Inputs:");
            slot_grid(ui, input_slots, icons);

            ui.add_space(4.0);

            // --- Output slots ---
            ui.label("Outputs:");
            slot_grid(ui, output_slots, icons);
        });

    if !open {
        return Some(MachineAction::Close);
    }

    action
}

/// Render a row of item slots.
fn slot_grid(
    ui: &mut egui::Ui,
    slots: &[crate::sim::machine::ItemStack; MAX_SLOTS],
    icons: &IconAtlas,
) {
    ui.horizontal(|ui| {
        for slot in slots.iter() {
            let (rect, _response) = ui.allocate_exact_size(
                egui::vec2(40.0, 40.0),
                egui::Sense::hover(),
            );

            // Slot background + border
            ui.painter().rect(
                rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(20, 20, 30, 200),
                egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 50, 80)),
                egui::StrokeKind::Inside,
            );

            if slot.count > 0 {
                // Draw item icon
                if let Some(tex) = icons.get(slot.item) {
                    let icon_rect = egui::Rect::from_center_size(
                        rect.center() - egui::vec2(0.0, 4.0),
                        egui::vec2(24.0, 24.0),
                    );
                    ui.painter().image(
                        tex.id(),
                        icon_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }

                // Draw count
                let count_text = format!("{}", slot.count);
                ui.painter().text(
                    rect.center() + egui::vec2(0.0, 12.0),
                    egui::Align2::CENTER_CENTER,
                    &count_text,
                    egui::FontId::proportional(10.0),
                    egui::Color32::from_rgb(200, 200, 220),
                );
            }
        }
    });
}

/// Format a recipe as a label for the dropdown.
fn recipe_label(recipe: &crate::game::items::Recipe) -> String {
    if recipe.inputs.is_empty() {
        // Source machine: just show the output item name
        return recipe.output.display_name().to_string();
    }
    let inputs: Vec<String> = recipe
        .inputs
        .iter()
        .map(|(id, count)| {
            if *count > 1 {
                format!("{}x {}", count, id.display_name())
            } else {
                id.display_name().to_string()
            }
        })
        .collect();
    format!(
        "{} -> {}",
        inputs.join(" + "),
        recipe.output.display_name()
    )
}

/// Status text and color for each machine state.
fn state_display(state: MachineState) -> (&'static str, egui::Color32) {
    match state {
        MachineState::Idle => ("Idle", egui::Color32::from_rgb(150, 150, 150)),
        MachineState::Working => ("Working", egui::Color32::from_rgb(100, 200, 100)),
        MachineState::OutputFull => ("Output Full", egui::Color32::from_rgb(230, 180, 50)),
        MachineState::NoInput => ("No Input", egui::Color32::from_rgb(200, 100, 100)),
        MachineState::NoPower => ("No Power", egui::Color32::from_rgb(200, 50, 50)),
    }
}
