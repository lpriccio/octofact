use crate::game::world::EntityId;
use crate::sim::belt::BeltNetwork;
use crate::sim::storage::StoragePool;

/// Actions the storage panel can produce.
pub enum StorageAction {
    /// User closed the panel.
    Close,
}

/// Draw the storage inspection panel. Returns an action if the user interacted.
pub fn storage_panel(
    ctx: &egui::Context,
    entity: EntityId,
    storage_pool: &StoragePool,
    belt_network: &BeltNetwork,
) -> Option<StorageAction> {
    let state = storage_pool.get(entity)?;

    let mut open = true;
    let (input_count, output_count) = belt_network.storage_connection_counts(entity);

    // Count total items stored
    let total_items: u32 = state.slots.iter().map(|s| s.count as u32).sum();
    let used_slots = state.slots.iter().filter(|s| s.count > 0).count();

    egui::Window::new("Storage")
        .open(&mut open)
        .collapsible(true)
        .resizable(false)
        .default_width(220.0)
        .show(ctx, |ui| {
            // --- Connections ---
            ui.horizontal(|ui| {
                ui.label("Connections:");
                if input_count == 0 && output_count == 0 {
                    ui.colored_label(
                        egui::Color32::from_rgb(150, 150, 150),
                        "None",
                    );
                } else {
                    if input_count > 0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 130, 255),
                            format!("{} in", input_count),
                        );
                    }
                    if output_count > 0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 153, 51),
                            format!("{} out", output_count),
                        );
                    }
                }
            });

            ui.separator();

            // --- Capacity ---
            ui.horizontal(|ui| {
                ui.label("Capacity:");
                ui.label(format!("{}/20 slots", used_slots));
            });

            if total_items > 0 {
                ui.separator();
                ui.label("Contents:");
                for slot in &state.slots {
                    if slot.count > 0 {
                        ui.horizontal(|ui| {
                            ui.label(format!("  {} x{}", slot.item.display_name(), slot.count));
                        });
                    }
                }
            }
        });

    if !open {
        return Some(StorageAction::Close);
    }

    None
}
