use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameAction {
    MoveForward,
    MoveBackward,
    StrafeLeft,
    StrafeRight,
    CameraUp,
    CameraDown,
    ToggleLabels,
    OpenSettings,
    OpenInventory,
    ToggleViewMode,
    PlaceStructure,
    RemoveStructure,
    RotateStructure,
    ToggleGrid,
}

impl GameAction {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::MoveForward => "Move Forward",
            Self::MoveBackward => "Move Backward",
            Self::StrafeLeft => "Strafe Left",
            Self::StrafeRight => "Strafe Right",
            Self::CameraUp => "Camera Up",
            Self::CameraDown => "Camera Down",
            Self::ToggleLabels => "Toggle Labels",
            Self::OpenSettings => "Settings",
            Self::OpenInventory => "Inventory",
            Self::ToggleViewMode => "Toggle View",
            Self::PlaceStructure => "Place Structure",
            Self::RemoveStructure => "Remove Structure",
            Self::RotateStructure => "Rotate Structure",
            Self::ToggleGrid => "Toggle Grid",
        }
    }

    pub fn all() -> &'static [GameAction] {
        use GameAction::*;
        &[
            MoveForward, MoveBackward, StrafeLeft, StrafeRight,
            CameraUp, CameraDown, ToggleLabels, OpenSettings,
            OpenInventory, ToggleViewMode, PlaceStructure,
            RemoveStructure, RotateStructure, ToggleGrid,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyBind {
    pub code: KeyCode,
}

impl KeyBind {
    pub fn new(code: KeyCode) -> Self {
        Self { code }
    }

    pub fn display_name(&self) -> String {
        format!("{:?}", self.code)
    }
}

impl Serialize for KeyBind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:?}", self.code))
    }
}

impl<'de> Deserialize<'de> for KeyBind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let code = keycode_from_str(&s).ok_or_else(|| {
            serde::de::Error::custom(format!("Unknown key code: {s}"))
        })?;
        Ok(KeyBind { code })
    }
}

fn keycode_from_str(s: &str) -> Option<KeyCode> {
    // Match the Debug output of KeyCode variants
    match s {
        "KeyA" => Some(KeyCode::KeyA),
        "KeyB" => Some(KeyCode::KeyB),
        "KeyC" => Some(KeyCode::KeyC),
        "KeyD" => Some(KeyCode::KeyD),
        "KeyE" => Some(KeyCode::KeyE),
        "KeyF" => Some(KeyCode::KeyF),
        "KeyG" => Some(KeyCode::KeyG),
        "KeyH" => Some(KeyCode::KeyH),
        "KeyI" => Some(KeyCode::KeyI),
        "KeyJ" => Some(KeyCode::KeyJ),
        "KeyK" => Some(KeyCode::KeyK),
        "KeyL" => Some(KeyCode::KeyL),
        "KeyM" => Some(KeyCode::KeyM),
        "KeyN" => Some(KeyCode::KeyN),
        "KeyO" => Some(KeyCode::KeyO),
        "KeyP" => Some(KeyCode::KeyP),
        "KeyQ" => Some(KeyCode::KeyQ),
        "KeyR" => Some(KeyCode::KeyR),
        "KeyS" => Some(KeyCode::KeyS),
        "KeyT" => Some(KeyCode::KeyT),
        "KeyU" => Some(KeyCode::KeyU),
        "KeyV" => Some(KeyCode::KeyV),
        "KeyW" => Some(KeyCode::KeyW),
        "KeyX" => Some(KeyCode::KeyX),
        "KeyY" => Some(KeyCode::KeyY),
        "KeyZ" => Some(KeyCode::KeyZ),
        "Digit0" => Some(KeyCode::Digit0),
        "Digit1" => Some(KeyCode::Digit1),
        "Digit2" => Some(KeyCode::Digit2),
        "Digit3" => Some(KeyCode::Digit3),
        "Digit4" => Some(KeyCode::Digit4),
        "Digit5" => Some(KeyCode::Digit5),
        "Digit6" => Some(KeyCode::Digit6),
        "Digit7" => Some(KeyCode::Digit7),
        "Digit8" => Some(KeyCode::Digit8),
        "Digit9" => Some(KeyCode::Digit9),
        "Escape" => Some(KeyCode::Escape),
        "Tab" => Some(KeyCode::Tab),
        "Space" => Some(KeyCode::Space),
        "Enter" => Some(KeyCode::Enter),
        "Backspace" => Some(KeyCode::Backspace),
        "ArrowUp" => Some(KeyCode::ArrowUp),
        "ArrowDown" => Some(KeyCode::ArrowDown),
        "ArrowLeft" => Some(KeyCode::ArrowLeft),
        "ArrowRight" => Some(KeyCode::ArrowRight),
        "Backquote" => Some(KeyCode::Backquote),
        "ShiftLeft" => Some(KeyCode::ShiftLeft),
        "ShiftRight" => Some(KeyCode::ShiftRight),
        "ControlLeft" => Some(KeyCode::ControlLeft),
        "ControlRight" => Some(KeyCode::ControlRight),
        "AltLeft" => Some(KeyCode::AltLeft),
        "AltRight" => Some(KeyCode::AltRight),
        "F1" => Some(KeyCode::F1),
        "F2" => Some(KeyCode::F2),
        "F3" => Some(KeyCode::F3),
        "F4" => Some(KeyCode::F4),
        "F5" => Some(KeyCode::F5),
        "F6" => Some(KeyCode::F6),
        "F7" => Some(KeyCode::F7),
        "F8" => Some(KeyCode::F8),
        "F9" => Some(KeyCode::F9),
        "F10" => Some(KeyCode::F10),
        "F11" => Some(KeyCode::F11),
        "F12" => Some(KeyCode::F12),
        _ => None,
    }
}

pub fn default_bindings() -> HashMap<GameAction, KeyBind> {
    use GameAction::*;
    HashMap::from([
        (MoveForward, KeyBind::new(KeyCode::KeyW)),
        (MoveBackward, KeyBind::new(KeyCode::KeyS)),
        (StrafeLeft, KeyBind::new(KeyCode::KeyA)),
        (StrafeRight, KeyBind::new(KeyCode::KeyD)),
        (CameraUp, KeyBind::new(KeyCode::KeyQ)),
        (CameraDown, KeyBind::new(KeyCode::KeyE)),
        (ToggleLabels, KeyBind::new(KeyCode::KeyL)),
        (OpenSettings, KeyBind::new(KeyCode::Escape)),
        (OpenInventory, KeyBind::new(KeyCode::Tab)),
        (ToggleViewMode, KeyBind::new(KeyCode::Backquote)),
        (RotateStructure, KeyBind::new(KeyCode::KeyR)),
        (ToggleGrid, KeyBind::new(KeyCode::KeyG)),
    ])
}

pub struct InputState {
    pub bindings: HashMap<GameAction, KeyBind>,
    reverse_map: HashMap<KeyCode, Vec<GameAction>>,
    active_actions: HashSet<GameAction>,
    just_pressed_actions: HashSet<GameAction>,
}

impl InputState {
    pub fn new(bindings: HashMap<GameAction, KeyBind>) -> Self {
        let reverse_map = build_reverse_map(&bindings);
        Self {
            bindings,
            reverse_map,
            active_actions: HashSet::new(),
            just_pressed_actions: HashSet::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(default_bindings())
    }

    pub fn on_key_event(&mut self, code: KeyCode, pressed: bool) {
        if let Some(actions) = self.reverse_map.get(&code) {
            for &action in actions {
                if pressed {
                    self.active_actions.insert(action);
                    self.just_pressed_actions.insert(action);
                } else {
                    self.active_actions.remove(&action);
                }
            }
        }
    }

    pub fn is_active(&self, action: GameAction) -> bool {
        self.active_actions.contains(&action)
    }

    pub fn just_pressed(&self, action: GameAction) -> bool {
        self.just_pressed_actions.contains(&action)
    }

    pub fn end_frame(&mut self) {
        self.just_pressed_actions.clear();
    }

    pub fn rebind(&mut self, action: GameAction, code: KeyCode) {
        self.bindings.insert(action, KeyBind::new(code));
        self.reverse_map = build_reverse_map(&self.bindings);
    }
}

fn build_reverse_map(bindings: &HashMap<GameAction, KeyBind>) -> HashMap<KeyCode, Vec<GameAction>> {
    let mut map: HashMap<KeyCode, Vec<GameAction>> = HashMap::new();
    for (&action, bind) in bindings {
        map.entry(bind.code).or_default().push(action);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bindings_cover_movement() {
        let bindings = default_bindings();
        assert!(bindings.contains_key(&GameAction::MoveForward));
        assert!(bindings.contains_key(&GameAction::MoveBackward));
        assert!(bindings.contains_key(&GameAction::StrafeLeft));
        assert!(bindings.contains_key(&GameAction::StrafeRight));
        assert!(bindings.contains_key(&GameAction::CameraUp));
        assert!(bindings.contains_key(&GameAction::CameraDown));
    }

    #[test]
    fn test_input_state_action_resolution() {
        let mut state = InputState::with_defaults();
        state.on_key_event(KeyCode::KeyW, true);
        assert!(state.is_active(GameAction::MoveForward));
        assert!(state.just_pressed(GameAction::MoveForward));

        state.end_frame();
        assert!(state.is_active(GameAction::MoveForward));
        assert!(!state.just_pressed(GameAction::MoveForward));

        state.on_key_event(KeyCode::KeyW, false);
        assert!(!state.is_active(GameAction::MoveForward));
    }

    #[test]
    fn test_rebinding() {
        let mut state = InputState::with_defaults();
        state.rebind(GameAction::MoveForward, KeyCode::ArrowUp);

        state.on_key_event(KeyCode::KeyW, true);
        assert!(!state.is_active(GameAction::MoveForward));

        state.on_key_event(KeyCode::ArrowUp, true);
        assert!(state.is_active(GameAction::MoveForward));
    }

    #[test]
    fn test_all_actions_listed() {
        assert_eq!(GameAction::all().len(), 14);
    }
}
