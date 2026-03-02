pub fn apply_octofact_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Near-black backgrounds with slight transparency
    let bg = egui::Color32::from_rgba_unmultiplied(8, 8, 18, 220);
    let accent = egui::Color32::from_rgb(102, 77, 179);
    let border = egui::Color32::from_rgb(60, 50, 80);
    let text_color = egui::Color32::from_rgb(220, 215, 235);

    style.visuals.window_fill = bg;
    style.visuals.panel_fill = bg;
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(4, 4, 10);

    // Zero rounding on all widgets
    style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
    style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
    for w in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        w.corner_radius = egui::CornerRadius::ZERO;
        w.fg_stroke.color = text_color;
    }

    // Accent on hovered/active
    style.visuals.widgets.hovered.bg_fill = accent.gamma_multiply(0.3);
    style.visuals.widgets.active.bg_fill = accent.gamma_multiply(0.5);
    style.visuals.selection.bg_fill = accent.gamma_multiply(0.4);

    // Window border
    style.visuals.window_stroke = egui::Stroke::new(1.0, border);
    style.visuals.window_shadow = egui::epaint::Shadow::NONE;

    // Override text color
    style.visuals.override_text_color = Some(text_color);

    ctx.set_style(style);
}
