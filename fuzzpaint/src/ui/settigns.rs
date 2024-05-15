fn egui_key_to_winit_key(key: egui::Key) -> winit::keyboard::KeyCode {
    use egui::Key as EKey;
    use winit::keyboard::KeyCode as WKey;
    // Adapted from egui_winit, nightmare match statement!
    match key {
        EKey::Tab => WKey::Tab,
        EKey::ArrowDown => WKey::ArrowDown,
        EKey::ArrowLeft => WKey::ArrowLeft,
        EKey::ArrowRight => WKey::ArrowRight,
        EKey::ArrowUp => WKey::ArrowUp,
        EKey::End => WKey::End,
        EKey::Home => WKey::Home,
        EKey::PageDown => WKey::PageDown,
        EKey::PageUp => WKey::PageUp,
        EKey::Backspace => WKey::Backspace,
        EKey::Delete => WKey::Delete,
        EKey::Insert => WKey::Insert,
        EKey::Escape => WKey::Escape,
        EKey::Cut => WKey::Cut,
        EKey::Copy => WKey::Copy,
        EKey::Paste => WKey::Paste,
        EKey::Space => WKey::Space,
        EKey::Enter => WKey::Enter,
        EKey::Comma => WKey::Comma,
        EKey::Period => WKey::Period,

        EKey::Colon | EKey::Semicolon => WKey::Semicolon,
        EKey::Pipe | EKey::Backslash => WKey::Backslash,
        EKey::Questionmark | EKey::Slash => WKey::Slash,

        EKey::OpenBracket => WKey::BracketLeft,
        EKey::CloseBracket => WKey::BracketRight,
        EKey::Backtick => WKey::Backquote,
        EKey::Minus => WKey::Minus,
        EKey::Plus => WKey::NumpadAdd,
        EKey::Equals => WKey::Equal,
        EKey::Num0 => WKey::Digit0,
        EKey::Num1 => WKey::Digit1,
        EKey::Num2 => WKey::Digit2,
        EKey::Num3 => WKey::Digit3,
        EKey::Num4 => WKey::Digit4,
        EKey::Num5 => WKey::Digit5,
        EKey::Num6 => WKey::Digit6,
        EKey::Num7 => WKey::Digit7,
        EKey::Num8 => WKey::Digit8,
        EKey::Num9 => WKey::Digit9,
        EKey::A => WKey::KeyA,
        EKey::B => WKey::KeyB,
        EKey::C => WKey::KeyC,
        EKey::D => WKey::KeyD,
        EKey::E => WKey::KeyE,
        EKey::F => WKey::KeyF,
        EKey::G => WKey::KeyG,
        EKey::H => WKey::KeyH,
        EKey::I => WKey::KeyI,
        EKey::J => WKey::KeyJ,
        EKey::K => WKey::KeyK,
        EKey::L => WKey::KeyL,
        EKey::M => WKey::KeyM,
        EKey::N => WKey::KeyN,
        EKey::O => WKey::KeyO,
        EKey::P => WKey::KeyP,
        EKey::Q => WKey::KeyQ,
        EKey::R => WKey::KeyR,
        EKey::S => WKey::KeyS,
        EKey::T => WKey::KeyT,
        EKey::U => WKey::KeyU,
        EKey::V => WKey::KeyV,
        EKey::W => WKey::KeyW,
        EKey::X => WKey::KeyX,
        EKey::Y => WKey::KeyY,
        EKey::Z => WKey::KeyZ,
        EKey::F1 => WKey::F1,
        EKey::F2 => WKey::F2,
        EKey::F3 => WKey::F3,
        EKey::F4 => WKey::F4,
        EKey::F5 => WKey::F5,
        EKey::F6 => WKey::F6,
        EKey::F7 => WKey::F7,
        EKey::F8 => WKey::F8,
        EKey::F9 => WKey::F9,
        EKey::F10 => WKey::F10,
        EKey::F11 => WKey::F11,
        EKey::F12 => WKey::F12,
        EKey::F13 => WKey::F13,
        EKey::F14 => WKey::F14,
        EKey::F15 => WKey::F15,
        EKey::F16 => WKey::F16,
        EKey::F17 => WKey::F17,
        EKey::F18 => WKey::F18,
        EKey::F19 => WKey::F19,
        EKey::F20 => WKey::F20,
    }
}

pub fn clicked_hotkey(input: &egui::InputState) -> Option<crate::actions::hotkeys::KeyboardHotkey> {
    // Find the first key clicked this frame.
    let (&key, &modifiers) = input.events.iter().find_map(|event| {
        if let egui::Event::Key {
            pressed: true,
            key,
            modifiers,
            ..
        } = event
        {
            Some((key, modifiers))
        } else {
            None
        }
    })?;

    let key = egui_key_to_winit_key(key);

    Some(crate::actions::hotkeys::KeyboardHotkey {
        alt: modifiers.alt,
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        key,
    })
}

pub fn test(ui: &mut egui::Ui) {
    let mut write = crate::global::hotkeys::Hotkeys::write();

    for action in <crate::actions::Action as strum::IntoEnumIterator>::iter() {
        ui.label(action.as_ref());
        ui.label(format!("{:#?}", write.actions_to_keys.get(action)));
    }
}

const ALL_KEYS: &[winit::keyboard::KeyCode] = &[
    winit::keyboard::KeyCode::Backquote,
    winit::keyboard::KeyCode::Backslash,
    winit::keyboard::KeyCode::BracketLeft,
    winit::keyboard::KeyCode::BracketRight,
    winit::keyboard::KeyCode::Comma,
    winit::keyboard::KeyCode::Digit0,
    winit::keyboard::KeyCode::Digit1,
    winit::keyboard::KeyCode::Digit2,
    winit::keyboard::KeyCode::Digit3,
    winit::keyboard::KeyCode::Digit4,
    winit::keyboard::KeyCode::Digit5,
    winit::keyboard::KeyCode::Digit6,
    winit::keyboard::KeyCode::Digit7,
    winit::keyboard::KeyCode::Digit8,
    winit::keyboard::KeyCode::Digit9,
    winit::keyboard::KeyCode::Equal,
    winit::keyboard::KeyCode::IntlBackslash,
    winit::keyboard::KeyCode::IntlRo,
    winit::keyboard::KeyCode::IntlYen,
    winit::keyboard::KeyCode::KeyA,
    winit::keyboard::KeyCode::KeyB,
    winit::keyboard::KeyCode::KeyC,
    winit::keyboard::KeyCode::KeyD,
    winit::keyboard::KeyCode::KeyE,
    winit::keyboard::KeyCode::KeyF,
    winit::keyboard::KeyCode::KeyG,
    winit::keyboard::KeyCode::KeyH,
    winit::keyboard::KeyCode::KeyI,
    winit::keyboard::KeyCode::KeyJ,
    winit::keyboard::KeyCode::KeyK,
    winit::keyboard::KeyCode::KeyL,
    winit::keyboard::KeyCode::KeyM,
    winit::keyboard::KeyCode::KeyN,
    winit::keyboard::KeyCode::KeyO,
    winit::keyboard::KeyCode::KeyP,
    winit::keyboard::KeyCode::KeyQ,
    winit::keyboard::KeyCode::KeyR,
    winit::keyboard::KeyCode::KeyS,
    winit::keyboard::KeyCode::KeyT,
    winit::keyboard::KeyCode::KeyU,
    winit::keyboard::KeyCode::KeyV,
    winit::keyboard::KeyCode::KeyW,
    winit::keyboard::KeyCode::KeyX,
    winit::keyboard::KeyCode::KeyY,
    winit::keyboard::KeyCode::KeyZ,
    winit::keyboard::KeyCode::Minus,
    winit::keyboard::KeyCode::Period,
    winit::keyboard::KeyCode::Quote,
    winit::keyboard::KeyCode::Semicolon,
    winit::keyboard::KeyCode::Slash,
    winit::keyboard::KeyCode::AltLeft,
    winit::keyboard::KeyCode::AltRight,
    winit::keyboard::KeyCode::Backspace,
    winit::keyboard::KeyCode::CapsLock,
    winit::keyboard::KeyCode::ContextMenu,
    winit::keyboard::KeyCode::ControlLeft,
    winit::keyboard::KeyCode::ControlRight,
    winit::keyboard::KeyCode::Enter,
    winit::keyboard::KeyCode::SuperLeft,
    winit::keyboard::KeyCode::SuperRight,
    winit::keyboard::KeyCode::ShiftLeft,
    winit::keyboard::KeyCode::ShiftRight,
    winit::keyboard::KeyCode::Space,
    winit::keyboard::KeyCode::Tab,
    winit::keyboard::KeyCode::Convert,
    winit::keyboard::KeyCode::KanaMode,
    winit::keyboard::KeyCode::Lang1,
    winit::keyboard::KeyCode::Lang2,
    winit::keyboard::KeyCode::Lang3,
    winit::keyboard::KeyCode::Lang4,
    winit::keyboard::KeyCode::Lang5,
    winit::keyboard::KeyCode::NonConvert,
    winit::keyboard::KeyCode::Delete,
    winit::keyboard::KeyCode::End,
    winit::keyboard::KeyCode::Help,
    winit::keyboard::KeyCode::Home,
    winit::keyboard::KeyCode::Insert,
    winit::keyboard::KeyCode::PageDown,
    winit::keyboard::KeyCode::PageUp,
    winit::keyboard::KeyCode::ArrowDown,
    winit::keyboard::KeyCode::ArrowLeft,
    winit::keyboard::KeyCode::ArrowRight,
    winit::keyboard::KeyCode::ArrowUp,
    winit::keyboard::KeyCode::NumLock,
    winit::keyboard::KeyCode::Numpad0,
    winit::keyboard::KeyCode::Numpad1,
    winit::keyboard::KeyCode::Numpad2,
    winit::keyboard::KeyCode::Numpad3,
    winit::keyboard::KeyCode::Numpad4,
    winit::keyboard::KeyCode::Numpad5,
    winit::keyboard::KeyCode::Numpad6,
    winit::keyboard::KeyCode::Numpad7,
    winit::keyboard::KeyCode::Numpad8,
    winit::keyboard::KeyCode::Numpad9,
    winit::keyboard::KeyCode::NumpadAdd,
    winit::keyboard::KeyCode::NumpadBackspace,
    winit::keyboard::KeyCode::NumpadClear,
    winit::keyboard::KeyCode::NumpadClearEntry,
    winit::keyboard::KeyCode::NumpadComma,
    winit::keyboard::KeyCode::NumpadDecimal,
    winit::keyboard::KeyCode::NumpadDivide,
    winit::keyboard::KeyCode::NumpadEnter,
    winit::keyboard::KeyCode::NumpadEqual,
    winit::keyboard::KeyCode::NumpadHash,
    winit::keyboard::KeyCode::NumpadMemoryAdd,
    winit::keyboard::KeyCode::NumpadMemoryClear,
    winit::keyboard::KeyCode::NumpadMemoryRecall,
    winit::keyboard::KeyCode::NumpadMemoryStore,
    winit::keyboard::KeyCode::NumpadMemorySubtract,
    winit::keyboard::KeyCode::NumpadMultiply,
    winit::keyboard::KeyCode::NumpadParenLeft,
    winit::keyboard::KeyCode::NumpadParenRight,
    winit::keyboard::KeyCode::NumpadStar,
    winit::keyboard::KeyCode::NumpadSubtract,
    winit::keyboard::KeyCode::Escape,
    winit::keyboard::KeyCode::Fn,
    winit::keyboard::KeyCode::FnLock,
    winit::keyboard::KeyCode::PrintScreen,
    winit::keyboard::KeyCode::ScrollLock,
    winit::keyboard::KeyCode::Pause,
    winit::keyboard::KeyCode::BrowserBack,
    winit::keyboard::KeyCode::BrowserFavorites,
    winit::keyboard::KeyCode::BrowserForward,
    winit::keyboard::KeyCode::BrowserHome,
    winit::keyboard::KeyCode::BrowserRefresh,
    winit::keyboard::KeyCode::BrowserSearch,
    winit::keyboard::KeyCode::BrowserStop,
    winit::keyboard::KeyCode::Eject,
    winit::keyboard::KeyCode::LaunchApp1,
    winit::keyboard::KeyCode::LaunchApp2,
    winit::keyboard::KeyCode::LaunchMail,
    winit::keyboard::KeyCode::MediaPlayPause,
    winit::keyboard::KeyCode::MediaSelect,
    winit::keyboard::KeyCode::MediaStop,
    winit::keyboard::KeyCode::MediaTrackNext,
    winit::keyboard::KeyCode::MediaTrackPrevious,
    winit::keyboard::KeyCode::Power,
    winit::keyboard::KeyCode::Sleep,
    winit::keyboard::KeyCode::AudioVolumeDown,
    winit::keyboard::KeyCode::AudioVolumeMute,
    winit::keyboard::KeyCode::AudioVolumeUp,
    winit::keyboard::KeyCode::WakeUp,
    winit::keyboard::KeyCode::Meta,
    winit::keyboard::KeyCode::Hyper,
    winit::keyboard::KeyCode::Turbo,
    winit::keyboard::KeyCode::Abort,
    winit::keyboard::KeyCode::Resume,
    winit::keyboard::KeyCode::Suspend,
    winit::keyboard::KeyCode::Again,
    winit::keyboard::KeyCode::Copy,
    winit::keyboard::KeyCode::Cut,
    winit::keyboard::KeyCode::Find,
    winit::keyboard::KeyCode::Open,
    winit::keyboard::KeyCode::Paste,
    winit::keyboard::KeyCode::Props,
    winit::keyboard::KeyCode::Select,
    winit::keyboard::KeyCode::Undo,
    winit::keyboard::KeyCode::Hiragana,
    winit::keyboard::KeyCode::Katakana,
    winit::keyboard::KeyCode::F1,
    winit::keyboard::KeyCode::F2,
    winit::keyboard::KeyCode::F3,
    winit::keyboard::KeyCode::F4,
    winit::keyboard::KeyCode::F5,
    winit::keyboard::KeyCode::F6,
    winit::keyboard::KeyCode::F7,
    winit::keyboard::KeyCode::F8,
    winit::keyboard::KeyCode::F9,
    winit::keyboard::KeyCode::F10,
    winit::keyboard::KeyCode::F11,
    winit::keyboard::KeyCode::F12,
    winit::keyboard::KeyCode::F13,
    winit::keyboard::KeyCode::F14,
    winit::keyboard::KeyCode::F15,
    winit::keyboard::KeyCode::F16,
    winit::keyboard::KeyCode::F17,
    winit::keyboard::KeyCode::F18,
    winit::keyboard::KeyCode::F19,
    winit::keyboard::KeyCode::F20,
    winit::keyboard::KeyCode::F21,
    winit::keyboard::KeyCode::F22,
    winit::keyboard::KeyCode::F23,
    winit::keyboard::KeyCode::F24,
    winit::keyboard::KeyCode::F25,
    winit::keyboard::KeyCode::F26,
    winit::keyboard::KeyCode::F27,
    winit::keyboard::KeyCode::F28,
    winit::keyboard::KeyCode::F29,
    winit::keyboard::KeyCode::F30,
    winit::keyboard::KeyCode::F31,
    winit::keyboard::KeyCode::F32,
    winit::keyboard::KeyCode::F33,
    winit::keyboard::KeyCode::F34,
    winit::keyboard::KeyCode::F35,
];
