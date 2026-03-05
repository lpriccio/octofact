use crate::game::config::{DiskEmbeddingConfig, GameConfig};
use crate::game::input::{GameAction, InputState};

/// Actions that the settings menu can request from the app.
pub enum SettingsAction {
    Save,
    SaveNamed(String),
    Load(std::path::PathBuf),
    DeleteSave(String),
}

pub fn settings_menu(
    ctx: &egui::Context,
    open: &mut bool,
    config: &mut GameConfig,
    input_state: &mut InputState,
    rebinding: &mut Option<GameAction>,
) -> Option<SettingsAction> {
    if !*open {
        return None;
    }

    let mut action = None;

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
                ui.selectable_value(&mut tab, SettingsTab::SaveLoad, "Save/Load");
                ui.selectable_value(&mut tab, SettingsTab::Debug, "Debug");
            });

            ui.data_mut(|d| d.insert_temp(tab_id, tab));

            ui.separator();

            match tab {
                SettingsTab::KeyBindings => {
                    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                        for &game_action in GameAction::all() {
                            ui.horizontal(|ui| {
                                ui.label(game_action.display_name());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let is_rebinding = *rebinding == Some(game_action);

                                    if is_rebinding {
                                        ui.label("Press a key...");
                                    } else {
                                        let key_name = input_state.bindings
                                            .get(&game_action)
                                            .map(|b| b.display_name())
                                            .unwrap_or_else(|| "Unbound".to_string());

                                        if ui.button(&key_name).clicked() {
                                            *rebinding = Some(game_action);
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

                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(egui::RichText::new("Disk Embedding").strong());

                    // Embedding type selector (only Paraboloid for now)
                    let embedding_label = match &config.graphics.disk_embedding {
                        DiskEmbeddingConfig::Paraboloid { .. } => "Paraboloid",
                    };
                    ui.horizontal(|ui| {
                        ui.label("Type:");
                        egui::ComboBox::from_id_salt("disk_embedding_type")
                            .selected_text(embedding_label)
                            .show_ui(ui, |ui| {
                                let is_paraboloid = matches!(config.graphics.disk_embedding, DiskEmbeddingConfig::Paraboloid { .. });
                                if ui.selectable_label(is_paraboloid, "Paraboloid").clicked() && !is_paraboloid {
                                    config.graphics.disk_embedding = DiskEmbeddingConfig::default();
                                }
                            });
                    });

                    // Per-embedding parameters
                    match &mut config.graphics.disk_embedding {
                        DiskEmbeddingConfig::Paraboloid { height } => {
                            let text_id = ui.id().with("bowl_height_text");
                            let mut text_val: String = ui.data_mut(|d| {
                                d.get_temp::<String>(text_id)
                                    .unwrap_or_else(|| format!("{:.4}", height))
                            });

                            ui.horizontal(|ui| {
                                ui.label("Height:");
                                let slider_resp = ui.add(
                                    egui::Slider::new(height, -1.0_f64..=1.0)
                                        .step_by(0.001)
                                        .max_decimals(4)
                                        .clamping(egui::SliderClamping::Never),
                                );
                                if slider_resp.changed() {
                                    text_val = format!("{:.4}", height);
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label("Exact:");
                                let text_resp = ui.add(
                                    egui::TextEdit::singleline(&mut text_val)
                                        .desired_width(80.0),
                                );
                                if text_resp.changed() {
                                    if let Ok(v) = text_val.parse::<f64>() {
                                        *height = v;
                                    }
                                }
                            });

                            ui.data_mut(|d| d.insert_temp(text_id, text_val));
                        }
                    }
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
                SettingsTab::SaveLoad => {
                    // Quick save/load
                    ui.horizontal(|ui| {
                        if ui.button("Quick Save (F5)").clicked() {
                            action = Some(SettingsAction::Save);
                        }
                    });
                    ui.add_space(8.0);

                    // Named save
                    let name_id = ui.id().with("save_name_input");
                    let mut save_name: String = ui.data_mut(|d| {
                        d.get_temp_mut_or(name_id, String::new()).clone()
                    });
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut save_name);
                        if ui.button("Save As").clicked() && !save_name.trim().is_empty() {
                            let clean_name = save_name.trim().to_string();
                            action = Some(SettingsAction::SaveNamed(clean_name));
                        }
                    });
                    ui.data_mut(|d| d.insert_temp(name_id, save_name));

                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(egui::RichText::new("Saved Games").strong());

                    let saves = crate::game::save::list_saves();
                    if saves.is_empty() {
                        ui.label(
                            egui::RichText::new("No saves found.")
                                .weak()
                                .italics(),
                        );
                    } else {
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for (name, path) in &saves {
                                ui.horizontal(|ui| {
                                    ui.label(name.as_str());
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if name != "autosave" && ui.small_button("Delete").clicked() {
                                            action = Some(SettingsAction::DeleteSave(name.clone()));
                                        }
                                        if ui.small_button("Load").clicked() {
                                            action = Some(SettingsAction::Load(path.clone()));
                                        }
                                    });
                                });
                            }
                        });
                    }
                }
                SettingsTab::Debug => {
                    ui.checkbox(&mut config.debug.log_clicks, "Log click interactions to console");
                    ui.checkbox(&mut config.debug.free_placement, "Free placement (ignore inventory)");
                }
            }

            ui.separator();

            if ui.button("Close").clicked() {
                *open = false;
                config.save();
            }
        });

    action
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    KeyBindings,
    Graphics,
    Gameplay,
    SaveLoad,
    Debug,
}
