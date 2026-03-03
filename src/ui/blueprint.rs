use std::path::PathBuf;

use crate::game::blueprint::{self, BlueprintFile, Clipboard};
use crate::game::world::{Direction, StructureKind};

use super::icons::IconAtlas;

/// Actions the blueprint manager can request from the app.
pub enum BlueprintAction {
    /// Load a saved blueprint into the clipboard.
    LoadToClipboard(PathBuf),
    /// Save the current clipboard as a named blueprint.
    SaveClipboard(String),
    /// Rename a saved blueprint file.
    Rename(PathBuf, String),
    /// Delete a saved blueprint file.
    Delete(PathBuf),
    /// Window was closed via the title-bar X button.
    Close,
}

/// Persistent state for the blueprint manager window.
pub struct BlueprintManagerState {
    /// Cached list of saved blueprints (path, file).
    cached_blueprints: Vec<(PathBuf, BlueprintFile)>,
    /// Whether the cache needs refreshing.
    dirty: bool,
    /// Index of the currently selected blueprint in the list.
    selected: Option<usize>,
    /// Text input buffer for the "save as" name field.
    save_name: String,
    /// Text input buffer for renaming.
    rename_buf: String,
    /// Whether we're in rename mode.
    renaming: bool,
    /// Whether we're confirming a delete.
    confirm_delete: bool,
}

impl BlueprintManagerState {
    pub fn new() -> Self {
        Self {
            cached_blueprints: Vec::new(),
            dirty: true,
            selected: None,
            save_name: String::new(),
            rename_buf: String::new(),
            renaming: false,
            confirm_delete: false,
        }
    }

    /// Mark the cache as stale so it refreshes next frame.
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    fn refresh_if_needed(&mut self) {
        if self.dirty {
            self.cached_blueprints = blueprint::list_blueprints();
            self.dirty = false;
            // Clamp selection
            if let Some(idx) = self.selected {
                if idx >= self.cached_blueprints.len() {
                    self.selected = if self.cached_blueprints.is_empty() {
                        None
                    } else {
                        Some(self.cached_blueprints.len() - 1)
                    };
                }
            }
        }
    }
}

/// Color for a `StructureKind` in the schematic preview, matching icon primary colors.
fn structure_color(kind: StructureKind) -> egui::Color32 {
    let [r, g, b] = kind.to_item().icon_params().primary_color;
    egui::Color32::from_rgb(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
    )
}

/// Draw a directional arrow character for belts.
fn direction_arrow(dir: Direction) -> &'static str {
    match dir {
        Direction::North => "\u{2191}",
        Direction::East => "\u{2192}",
        Direction::South => "\u{2193}",
        Direction::West => "\u{2190}",
    }
}

/// Render the blueprint manager window. Returns an action if the user interacted.
pub fn blueprint_manager(
    ctx: &egui::Context,
    open: &mut bool,
    state: &mut BlueprintManagerState,
    clipboard: Option<&Clipboard>,
    tiling_q: u32,
    icons: &IconAtlas,
) -> Option<BlueprintAction> {
    if !*open {
        return None;
    }

    state.refresh_if_needed();

    let mut action = None;
    let mut still_open = true;

    egui::Window::new("Blueprint Manager")
        .open(&mut still_open)
        .collapsible(true)
        .resizable(true)
        .default_width(560.0)
        .default_height(420.0)
        .show(ctx, |ui| {
            // Main layout: left list + right preview
            ui.horizontal_top(|ui| {
                // Left panel: blueprint list
                ui.vertical(|ui| {
                    ui.set_min_width(220.0);
                    ui.set_max_width(220.0);
                    ui.heading("Saved Blueprints");
                    ui.separator();

                    if state.cached_blueprints.is_empty() {
                        ui.label("No blueprints saved yet.");
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(280.0)
                            .show(ui, |ui| {
                                let mut clicked_idx = None;
                                for (i, (_path, bp)) in state.cached_blueprints.iter().enumerate() {
                                    let selected = state.selected == Some(i);
                                    let label = format!(
                                        "{} ({}x{}, {} ent.)",
                                        bp.name,
                                        bp.width,
                                        bp.height,
                                        bp.entries.len(),
                                    );
                                    if ui.selectable_label(selected, &label).clicked() {
                                        clicked_idx = Some(i);
                                    }
                                }
                                if let Some(idx) = clicked_idx {
                                    state.selected = Some(idx);
                                    state.renaming = false;
                                    state.confirm_delete = false;
                                }
                            });
                    }

                    ui.separator();

                    // Action buttons
                    ui.horizontal(|ui| {
                        let has_selection = state.selected.is_some();

                        if ui
                            .add_enabled(has_selection, egui::Button::new("Load"))
                            .on_hover_text("Load into clipboard (Ctrl+V to paste)")
                            .clicked()
                        {
                            if let Some(idx) = state.selected {
                                let path = state.cached_blueprints[idx].0.clone();
                                action = Some(BlueprintAction::LoadToClipboard(path));
                            }
                        }

                        if ui
                            .add_enabled(has_selection && !state.renaming, egui::Button::new("Rename"))
                            .clicked()
                        {
                            if let Some(idx) = state.selected {
                                state.rename_buf = state.cached_blueprints[idx].1.name.clone();
                                state.renaming = true;
                                state.confirm_delete = false;
                            }
                        }

                        if ui
                            .add_enabled(has_selection && !state.confirm_delete, egui::Button::new("Delete"))
                            .clicked()
                        {
                            state.confirm_delete = true;
                            state.renaming = false;
                        }
                    });

                    // Rename inline input
                    if state.renaming {
                        ui.horizontal(|ui| {
                            ui.label("New name:");
                            let response = ui.text_edit_singleline(&mut state.rename_buf);
                            if (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                                || ui.button("OK").clicked()
                            {
                                if let Some(idx) = state.selected {
                                    let path = state.cached_blueprints[idx].0.clone();
                                    let new_name = state.rename_buf.clone();
                                    if !new_name.trim().is_empty() {
                                        action = Some(BlueprintAction::Rename(path, new_name));
                                    }
                                }
                                state.renaming = false;
                            }
                            if ui.button("Cancel").clicked() {
                                state.renaming = false;
                            }
                        });
                    }

                    // Delete confirmation
                    if state.confirm_delete {
                        ui.horizontal(|ui| {
                            ui.colored_label(egui::Color32::from_rgb(200, 50, 50), "Delete?");
                            if ui.button("Yes").clicked() {
                                if let Some(idx) = state.selected {
                                    let path = state.cached_blueprints[idx].0.clone();
                                    action = Some(BlueprintAction::Delete(path));
                                }
                                state.confirm_delete = false;
                            }
                            if ui.button("No").clicked() {
                                state.confirm_delete = false;
                            }
                        });
                    }

                    ui.separator();

                    // Save current clipboard
                    let has_clipboard = clipboard.is_some();
                    ui.label("Save clipboard as:");
                    ui.horizontal(|ui| {
                        ui.add_enabled(
                            has_clipboard,
                            egui::TextEdit::singleline(&mut state.save_name).desired_width(140.0),
                        );
                        if ui
                            .add_enabled(
                                has_clipboard && !state.save_name.trim().is_empty(),
                                egui::Button::new("Save"),
                            )
                            .clicked()
                        {
                            let name = state.save_name.trim().to_string();
                            action = Some(BlueprintAction::SaveClipboard(name));
                            state.save_name.clear();
                        }
                    });
                    if !has_clipboard {
                        ui.small("Ctrl+C to copy structures first");
                    }
                });

                ui.separator();

                // Right panel: preview + cost
                ui.vertical(|ui| {
                    ui.set_min_width(300.0);

                    if let Some(idx) = state.selected {
                        let bp = &state.cached_blueprints[idx].1;
                        draw_schematic(ui, bp);

                        ui.separator();

                        // Item cost summary
                        ui.label("Required items:");
                        let temp_clip = bp.to_clipboard();
                        let costs = blueprint::required_items(&temp_clip);
                        if costs.is_empty() {
                            ui.small("(empty blueprint)");
                        } else {
                            egui::Grid::new("bp_cost_grid")
                                .striped(true)
                                .spacing([8.0, 2.0])
                                .show(ui, |ui| {
                                    for (item_id, count) in &costs {
                                        ui.horizontal(|ui| {
                                            if let Some(tex) = icons.get(*item_id) {
                                                ui.image(egui::load::SizedTexture::new(
                                                    tex.id(),
                                                    egui::vec2(16.0, 16.0),
                                                ));
                                            }
                                            ui.label(item_id.display_name());
                                        });
                                        ui.label(format!("x{count}"));
                                        ui.end_row();
                                    }
                                });
                        }

                        // Tiling compatibility note
                        if bp.tiling_q != tiling_q {
                            ui.colored_label(
                                egui::Color32::from_rgb(200, 50, 50),
                                format!(
                                    "Incompatible: saved in {{4,{}}}, current is {{4,{}}}",
                                    bp.tiling_q, tiling_q
                                ),
                            );
                        }
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Select a blueprint to preview");
                        });
                    }
                });
            });

            // Footer: keybind hints
            ui.separator();
            let modifier = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
            ui.small(format!(
                "{modifier}+C copy | {modifier}+X cut | {modifier}+V paste | R rotate | B select"
            ));
        });

    if !still_open {
        action = Some(BlueprintAction::Close);
    }

    action
}

/// Draw a 2D schematic preview of a blueprint using egui::Painter.
fn draw_schematic(ui: &mut egui::Ui, bp: &BlueprintFile) {
    if bp.entries.is_empty() {
        ui.label("(empty blueprint)");
        return;
    }

    let bp_w = bp.width.max(1) as f32;
    let bp_h = bp.height.max(1) as f32;

    // Compute scale to fit within the available area, maintaining aspect ratio
    let available = ui.available_size();
    let max_w = available.x.min(300.0);
    let max_h = 200.0_f32;
    let cell_size = (max_w / bp_w).min(max_h / bp_h).clamp(2.0, 16.0);
    let canvas_w = bp_w * cell_size;
    let canvas_h = bp_h * cell_size;

    let (response, painter) =
        ui.allocate_painter(egui::vec2(canvas_w, canvas_h), egui::Sense::hover());
    let origin = response.rect.min;

    // Background
    painter.rect_filled(
        response.rect,
        2.0,
        egui::Color32::from_gray(30),
    );

    // Grid lines (subtle)
    let grid_color = egui::Color32::from_gray(50);
    for gx in 0..=(bp.width as i32) {
        let x = origin.x + gx as f32 * cell_size;
        painter.line_segment(
            [egui::pos2(x, origin.y), egui::pos2(x, origin.y + canvas_h)],
            egui::Stroke::new(0.5, grid_color),
        );
    }
    for gy in 0..=(bp.height as i32) {
        let y = origin.y + gy as f32 * cell_size;
        painter.line_segment(
            [egui::pos2(origin.x, y), egui::pos2(origin.x + canvas_w, y)],
            egui::Stroke::new(0.5, grid_color),
        );
    }

    // Draw structures
    for entry in &bp.entries {
        let color = structure_color(entry.kind);
        let (fw, fh) = entry.kind.footprint();
        let (rw, rh) = entry.direction.rotate_footprint(fw, fh);

        let x = origin.x + entry.offset.0 as f32 * cell_size;
        let y = origin.y + entry.offset.1 as f32 * cell_size;
        let w = rw as f32 * cell_size;
        let h = rh as f32 * cell_size;

        let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
        painter.rect_filled(rect.shrink(0.5), 1.0, color);

        // Belt direction arrow
        if entry.kind == StructureKind::Belt && cell_size >= 6.0 {
            let center = rect.center();
            let arrow = direction_arrow(entry.direction);
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                arrow,
                egui::FontId::proportional(cell_size * 0.7),
                egui::Color32::WHITE,
            );
        }
    }
}
