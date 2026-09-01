//! Allocation-free conversion from physical SDL locations to stock key text.

use sdl3::keyboard::Scancode as SdlScanCode;

use crate::platform::{KeyModifiers, MouseButton, ScanCode};

/// Maximum serialized chord size admitted by the known stock vocabulary.
const MAX_CHORD_BYTES: usize = 64;

/// Stack-owned stock chord used only for an immediate hash-table probe.
pub(super) struct StockChord {
    bytes: [u8; MAX_CHORD_BYTES],
    length: usize,
}

impl StockChord {
    /// Serializes generic modifiers in stock's `ALT-CTRL-SHIFT-` order.
    pub(super) fn keyboard(modifiers: KeyModifiers, scan_code: ScanCode) -> Option<Self> {
        Self::new(modifiers, keyboard_name(scan_code)?)
    }

    /// Serializes one of the five stock mouse-button names.
    pub(super) fn mouse(modifiers: KeyModifiers, button: MouseButton) -> Option<Self> {
        let name = match button {
            MouseButton::Left => "BUTTON1",
            MouseButton::Right => "BUTTON2",
            MouseButton::Middle => "BUTTON3",
            MouseButton::AuxiliaryOne => "BUTTON4",
            MouseButton::AuxiliaryTwo => "BUTTON5",
            MouseButton::Unknown => return None,
        };
        Self::new(modifiers, name)
    }

    /// Serializes one already-normalized vertical wheel transition.
    pub(super) fn wheel(modifiers: KeyModifiers, upward: bool) -> Option<Self> {
        Self::new(
            modifiers,
            if upward {
                "MOUSEWHEELUP"
            } else {
                "MOUSEWHEELDOWN"
            },
        )
    }

    /// Returns the temporary exact key string.
    pub(super) fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes[..self.length]).ok()
    }

    /// Appends only fixed ASCII tokens, with no heap allocation in the hot path.
    fn new(modifiers: KeyModifiers, base: &str) -> Option<Self> {
        let mut chord = Self {
            bytes: [0; MAX_CHORD_BYTES],
            length: 0,
        };
        if modifiers.has_alt() {
            chord.append("ALT-")?;
        }
        if modifiers.has_control() {
            chord.append("CTRL-")?;
        }
        if modifiers.has_shift() {
            chord.append("SHIFT-")?;
        }
        chord.append(base)?;
        Some(chord)
    }

    fn append(&mut self, value: &str) -> Option<()> {
        let end = self.length.checked_add(value.len())?;
        let output = self.bytes.get_mut(self.length..end)?;
        output.copy_from_slice(value.as_bytes());
        self.length = end;
        Some(())
    }
}

/// Maps physical locations whose exact build-12340 serialized names are known.
pub(crate) fn keyboard_name(scan_code: ScanCode) -> Option<&'static str> {
    let scan_code = SdlScanCode::from_i32(scan_code.value())?;
    Some(match scan_code {
        SdlScanCode::A => "A",
        SdlScanCode::B => "B",
        SdlScanCode::C => "C",
        SdlScanCode::D => "D",
        SdlScanCode::E => "E",
        SdlScanCode::F => "F",
        SdlScanCode::G => "G",
        SdlScanCode::H => "H",
        SdlScanCode::I => "I",
        SdlScanCode::J => "J",
        SdlScanCode::K => "K",
        SdlScanCode::L => "L",
        SdlScanCode::M => "M",
        SdlScanCode::N => "N",
        SdlScanCode::O => "O",
        SdlScanCode::P => "P",
        SdlScanCode::Q => "Q",
        SdlScanCode::R => "R",
        SdlScanCode::S => "S",
        SdlScanCode::T => "T",
        SdlScanCode::U => "U",
        SdlScanCode::V => "V",
        SdlScanCode::W => "W",
        SdlScanCode::X => "X",
        SdlScanCode::Y => "Y",
        SdlScanCode::Z => "Z",
        SdlScanCode::_0 => "0",
        SdlScanCode::_1 => "1",
        SdlScanCode::_2 => "2",
        SdlScanCode::_3 => "3",
        SdlScanCode::_4 => "4",
        SdlScanCode::_5 => "5",
        SdlScanCode::_6 => "6",
        SdlScanCode::_7 => "7",
        SdlScanCode::_8 => "8",
        SdlScanCode::_9 => "9",
        SdlScanCode::Return => "ENTER",
        SdlScanCode::Escape => "ESCAPE",
        SdlScanCode::Backspace => "BACKSPACE",
        SdlScanCode::Tab => "TAB",
        SdlScanCode::Space => "SPACE",
        SdlScanCode::Minus => "-",
        SdlScanCode::Equals => "=",
        SdlScanCode::LeftBracket => "[",
        SdlScanCode::RightBracket => "]",
        SdlScanCode::Backslash => "\\",
        SdlScanCode::Semicolon => ";",
        SdlScanCode::Apostrophe => "'",
        SdlScanCode::Grave => "`",
        SdlScanCode::Comma => ",",
        SdlScanCode::Period => ".",
        SdlScanCode::Slash => "/",
        SdlScanCode::F1 => "F1",
        SdlScanCode::F2 => "F2",
        SdlScanCode::F3 => "F3",
        SdlScanCode::F4 => "F4",
        SdlScanCode::F5 => "F5",
        SdlScanCode::F6 => "F6",
        SdlScanCode::F7 => "F7",
        SdlScanCode::F8 => "F8",
        SdlScanCode::F9 => "F9",
        SdlScanCode::F10 => "F10",
        SdlScanCode::F11 => "F11",
        SdlScanCode::F12 => "F12",
        SdlScanCode::F13 => "F13",
        SdlScanCode::F14 => "F14",
        SdlScanCode::F15 => "F15",
        SdlScanCode::F16 => "F16",
        SdlScanCode::F17 => "F17",
        SdlScanCode::F18 => "F18",
        SdlScanCode::F19 => "F19",
        SdlScanCode::F20 => "F20",
        SdlScanCode::F21 => "F21",
        SdlScanCode::F22 => "F22",
        SdlScanCode::F23 => "F23",
        SdlScanCode::F24 => "F24",
        SdlScanCode::PrintScreen => "PRINTSCREEN",
        SdlScanCode::ScrollLock => "SCROLLLOCK",
        SdlScanCode::Pause => "PAUSE",
        SdlScanCode::Insert => "INSERT",
        SdlScanCode::Home => "HOME",
        SdlScanCode::PageUp => "PAGEUP",
        SdlScanCode::Delete => "DELETE",
        SdlScanCode::End => "END",
        SdlScanCode::PageDown => "PAGEDOWN",
        SdlScanCode::Right => "RIGHT",
        SdlScanCode::Left => "LEFT",
        SdlScanCode::Down => "DOWN",
        SdlScanCode::Up => "UP",
        SdlScanCode::NumLockClear => "NUMLOCK",
        SdlScanCode::KpDivide => "NUMPADDIVIDE",
        SdlScanCode::KpMultiply => "NUMPADMULTIPLY",
        SdlScanCode::KpMinus => "NUMPADMINUS",
        SdlScanCode::KpPlus => "NUMPADPLUS",
        SdlScanCode::KpEnter => "NUMPADENTER",
        SdlScanCode::Kp0 => "NUMPAD0",
        SdlScanCode::Kp1 => "NUMPAD1",
        SdlScanCode::Kp2 => "NUMPAD2",
        SdlScanCode::Kp3 => "NUMPAD3",
        SdlScanCode::Kp4 => "NUMPAD4",
        SdlScanCode::Kp5 => "NUMPAD5",
        SdlScanCode::Kp6 => "NUMPAD6",
        SdlScanCode::Kp7 => "NUMPAD7",
        SdlScanCode::Kp8 => "NUMPAD8",
        SdlScanCode::Kp9 => "NUMPAD9",
        SdlScanCode::KpPeriod => "NUMPADDECIMAL",
        _ => return None,
    })
}
