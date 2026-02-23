use crate::game::input::GameAction;

pub fn keybind_button(
    ui: &mut egui::Ui,
    _action: GameAction,
    current_key: &str,
    is_rebinding: bool,
) -> egui::Response {
    if is_rebinding {
        ui.button("Press a key...")
    } else {
        ui.button(current_key)
    }
}
