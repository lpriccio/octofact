use crate::game::world::EntityId;
use crate::sim::belt::BeltNetwork;
use crate::sim::storage::{StoragePool, STORAGE_SLOTS, STORAGE_STACK_SIZE};

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

            // --- Capacity bar ---
            let max_items = (STORAGE_SLOTS as u32) * (STORAGE_STACK_SIZE as u32);
            let fill_frac = total_items as f32 / max_items as f32;
            ui.horizontal(|ui| {
                ui.label("Capacity:");
                ui.label(format!("{}/{}", total_items, max_items));
            });
            let bar_color = if fill_frac > 0.9 {
                egui::Color32::from_rgb(220, 80, 80)   // red when nearly full
            } else if fill_frac > 0.7 {
                egui::Color32::from_rgb(220, 180, 60)   // yellow when getting full
            } else {
                egui::Color32::from_rgb(80, 160, 80)    // green otherwise
            };
            let bar = egui::ProgressBar::new(fill_frac)
                .text(format!("{:.0}%", fill_frac * 100.0))
                .fill(bar_color);
            ui.add(bar);

            if total_items > 0 {
                ui.separator();
                ui.label(format!("Contents ({}/20 slots):", used_slots));
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
