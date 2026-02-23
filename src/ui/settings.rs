use crate::game::config::GameConfig;
use crate::game::input::{GameAction, InputState};

pub fn settings_menu(
    ctx: &egui::Context,
    open: &mut bool,
    config: &mut GameConfig,
    input_state: &mut InputState,
    rebinding: &mut Option<GameAction>,
) {
    if !*open {
        return;
    }

    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(400.0)
        .show(ctx, |ui| {
            let tab_id = ui.id().with("settings_tab");
            let mut tab: SettingsTab = ui.data_mut(|d| {
                *d.get_temp_mut_or(tab_id, SettingsTab::KeyBindings)
            });

            ui.horizontal(|ui| {
                ui.selectable_value(&mut tab, SettingsTab::KeyBindings, "Key Bindings");
                ui.selectable_value(&mut tab, SettingsTab::Graphics, "Graphics");
                ui.selectable_value(&mut tab, SettingsTab::Gameplay, "Gameplay");
                ui.selectable_value(&mut tab, SettingsTab::Debug, "Debug");
            });

            ui.data_mut(|d| d.insert_temp(tab_id, tab));

            ui.separator();

            match tab {
                SettingsTab::KeyBindings => {
                    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                        for &action in GameAction::all() {
                            ui.horizontal(|ui| {
                                ui.label(action.display_name());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let is_rebinding = *rebinding == Some(action);

                                    if is_rebinding {
                                        ui.label("Press a key...");
                                    } else {
                                        let key_name = input_state.bindings
                                            .get(&action)
                                            .map(|b| b.display_name())
                                            .unwrap_or_else(|| "Unbound".to_string());

                                        if ui.button(&key_name).clicked() {
                                            *rebinding = Some(action);
                                        }
                                    }
                                });
                            });
                        }
                    });
                }
                SettingsTab::Graphics => {
                    ui.horizontal(|ui| {
                        ui.label("Render Distance:");
                        ui.add(egui::Slider::new(&mut config.graphics.render_distance, 1..=6));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Frame Rate Cap:");
                        ui.add(egui::Slider::new(&mut config.graphics.frame_rate_cap, 30..=144));
                    });
                }
                SettingsTab::Gameplay => {
                    ui.horizontal(|ui| {
                        ui.label("Tiling n (in {4,n}):");
                        ui.add_enabled(false, egui::Slider::new(&mut config.gameplay.tiling_n, 5..=8));
                    });
                    ui.label(
                        egui::RichText::new("Determined by in-game tiling.")
                            .small()
                            .weak(),
                    );
                }
                SettingsTab::Debug => {
                    ui.checkbox(&mut config.debug.log_clicks, "Log click interactions to console");
                }
            }

            ui.separator();

            if ui.button("Close").clicked() {
                *open = false;
                config.save();
            }
        });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    KeyBindings,
    Graphics,
    Gameplay,
    Debug,
}
