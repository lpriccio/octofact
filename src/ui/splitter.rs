use crate::game::world::{Direction, EntityId, WorldState};
use crate::sim::splitter::{SplitterMode, SplitterPool};

/// Actions the splitter panel can produce.
pub enum SplitterAction {
    /// User closed the panel.
    Close,
}

/// Draw the splitter inspection panel. Returns an action if the user interacted.
pub fn splitter_panel(
    ctx: &egui::Context,
    entity: EntityId,
    splitter_pool: &SplitterPool,
    world: &WorldState,
) -> Option<SplitterAction> {
    let state = splitter_pool.get(entity)?;

    let mut open = true;

    egui::Window::new("Splitter")
        .open(&mut open)
        .collapsible(true)
        .resizable(false)
        .default_width(220.0)
        .show(ctx, |ui| {
            // --- Mode ---
            ui.horizontal(|ui| {
                ui.label("Mode:");
                let (mode_text, mode_color) = mode_display(state.mode);
                ui.colored_label(mode_color, mode_text);
            });

            ui.separator();

            // --- Connections ---
            ui.label("Connections:");

            if state.inputs.is_empty() && state.outputs.is_empty() {
                ui.colored_label(
                    egui::Color32::from_rgb(150, 150, 150),
                    "No belts connected",
                );
            } else {
                // Inputs
                if !state.inputs.is_empty() {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 130, 255),
                            format!("  {} input{}", state.inputs.len(), if state.inputs.len() == 1 { "" } else { "s" }),
                        );
                        let dirs: Vec<String> = state.inputs.iter()
                            .filter_map(|&belt| belt_direction_label(belt, entity, world))
                            .collect();
                        if !dirs.is_empty() {
                            ui.label(format!("({})", dirs.join(", ")));
                        }
                    });
                }

                // Outputs
                if !state.outputs.is_empty() {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 153, 51),
                            format!("  {} output{}", state.outputs.len(), if state.outputs.len() == 1 { "" } else { "s" }),
                        );
                        let dirs: Vec<String> = state.outputs.iter()
                            .filter_map(|&belt| belt_direction_label(belt, entity, world))
                            .collect();
                        if !dirs.is_empty() {
                            ui.label(format!("({})", dirs.join(", ")));
                        }
                    });
                }
            }
        });

    if !open {
        return Some(SplitterAction::Close);
    }

    None
}

/// Determine which side of the splitter a belt is on, return as a label like "N", "E", "S", "W".
fn belt_direction_label(belt: EntityId, splitter: EntityId, world: &WorldState) -> Option<String> {
    let bpos = world.position(belt)?;
    let spos = world.position(splitter)?;
    let (dx, dy) = (bpos.gx as i32 - spos.gx as i32, bpos.gy as i32 - spos.gy as i32);
    let dir = match (dx, dy) {
        (0, -1) => Direction::North,
        (1, 0) => Direction::East,
        (0, 1) => Direction::South,
        (-1, 0) => Direction::West,
        _ => return None,
    };
    Some(format!("{}", dir.arrow_char()))
}

/// Mode text and color for display.
fn mode_display(mode: SplitterMode) -> (&'static str, egui::Color32) {
    match mode {
        SplitterMode::Inactive => ("Inactive", egui::Color32::from_rgb(150, 150, 150)),
        SplitterMode::Merger => ("Merger", egui::Color32::from_rgb(80, 130, 255)),
        SplitterMode::Splitter => ("Splitter", egui::Color32::from_rgb(255, 153, 51)),
        SplitterMode::Balancer => ("Balancer", egui::Color32::from_rgb(100, 200, 100)),
    }
}
