use std::{
    path::PathBuf,
    str::FromStr,
    sync::{Mutex, MutexGuard},
};

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use crate::{
    events::{UiEvent, emit_ui_event},
    insert,
    live_text::LiveTextStore,
};

const MODIFIER_CODE_PREFIXES: [&str; 4] = ["Control", "Shift", "Alt", "Meta"];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    #[error("a shortcut needs at least one of Ctrl, Alt, Shift or Win/Cmd")]
    NoModifier,
    #[error("a shortcut needs a key besides the modifiers")]
    NoKey,
    #[error("{0} is a modifier, not a key")]
    OnlyModifiers(String),
    #[error("cannot read the shortcut \"{0}\"")]
    Unreadable(String),
}

pub struct InsertShortcutState {
    pub(crate) hotkey: Hotkey,
    pub(crate) registered: Option<Shortcut>,
    pub(crate) error: Option<String>,
}

pub struct InsertShortcut {
    pub(crate) state: Mutex<InsertShortcutState>,
    pub(crate) file: PathBuf,
}

impl InsertShortcut {
    /// Falls back to the default when nothing usable was saved.
    pub fn load(file: PathBuf) -> Self {
        let hotkey = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| Hotkey::parse(&text).ok())
            .unwrap_or_else(Hotkey::default_insert);
        Self {
            state: Mutex::new(InsertShortcutState {
                hotkey,
                registered: None,
                error: None,
            }),
            file,
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, InsertShortcutState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn fail(&self, message: String) {
        self.lock().error = Some(message);
    }

    pub fn register_saved(&self, app: &AppHandle) {
        let mut state = self.lock();
        let hotkey = state.hotkey.clone();
        state.error = swap_shortcut(app, &mut state, hotkey).err();
    }
}

fn to_shortcut(hotkey: &Hotkey) -> Result<Shortcut, String> {
    let mut modifiers = Modifiers::empty();
    for (held, modifier) in [
        (hotkey.ctrl, Modifiers::CONTROL),
        (hotkey.alt, Modifiers::ALT),
        (hotkey.shift, Modifiers::SHIFT),
        (hotkey.meta, Modifiers::SUPER),
    ] {
        if held {
            modifiers |= modifier;
        }
    }
    let code = Code::from_str(&hotkey.key)
        .map_err(|_| format!("{} is not a key that can be used", hotkey.key))?;
    Ok(Shortcut::new(Some(modifiers), code))
}

pub fn swap_shortcut(
    app: &AppHandle,
    state: &mut InsertShortcutState,
    hotkey: Hotkey,
) -> Result<(), String> {
    let shortcut = to_shortcut(&hotkey)?;
    let global = app.global_shortcut();

    let previous = state.registered.take();
    if let Some(previous) = previous {
        let _ = global.unregister(previous);
    }
    match global.register(shortcut) {
        Ok(()) => {
            state.registered = Some(shortcut);
            state.hotkey = hotkey;
            Ok(())
        }
        Err(error) => {
            if let Some(previous) = previous {
                if global.register(previous).is_ok() {
                    state.registered = Some(previous);
                }
            }
            Err(format!("{} is unavailable: {error}", hotkey.label()))
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Hotkey {
    pub(crate) ctrl: bool,
    pub(crate) alt: bool,
    pub(crate) shift: bool,
    pub(crate) meta: bool,
    pub(crate) key: String,
}

impl Hotkey {
    pub fn new(
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
        key: &str,
    ) -> Result<Self, HotkeyError> {
        if key.is_empty() {
            return Err(HotkeyError::NoKey);
        }
        if MODIFIER_CODE_PREFIXES
            .iter()
            .any(|prefix| key.starts_with(prefix))
        {
            return Err(HotkeyError::OnlyModifiers(key.to_owned()));
        }
        // Without a modifier the shortcut would swallow that key in every application.
        if !(ctrl || alt || shift || meta) {
            return Err(HotkeyError::NoModifier);
        }
        Ok(Self {
            ctrl,
            alt,
            shift,
            meta,
            key: key.to_owned(),
        })
    }

    pub fn default_insert() -> Self {
        Self {
            ctrl: true,
            alt: false,
            shift: true,
            meta: false,
            key: "Space".to_owned(),
        }
    }

    /// Human readable and parseable again by [`Hotkey::parse`], so it doubles as the stored form.
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        for (held, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.meta, "Meta"),
        ] {
            if held {
                parts.push(name.to_owned());
            }
        }
        parts.push(key_label(&self.key));
        parts.join("+")
    }

    pub fn parse(text: &str) -> Result<Self, HotkeyError> {
        let unreadable = || HotkeyError::Unreadable(text.trim().to_owned());
        let mut parts: Vec<&str> = text.trim().split('+').map(str::trim).collect();
        let key = parts
            .pop()
            .filter(|key| !key.is_empty())
            .ok_or_else(unreadable)?;

        let (mut ctrl, mut alt, mut shift, mut meta) = (false, false, false, false);
        for modifier in parts {
            match modifier {
                "Ctrl" => ctrl = true,
                "Alt" => alt = true,
                "Shift" => shift = true,
                "Meta" => meta = true,
                _ => return Err(unreadable()),
            }
        }
        Self::new(ctrl, alt, shift, meta, &key_code(key))
    }
}

fn key_label(code: &str) -> String {
    match code
        .strip_prefix("Key")
        .or_else(|| code.strip_prefix("Digit"))
    {
        Some(rest) if rest.len() == 1 => rest.to_owned(),
        _ => code.to_owned(),
    }
}

fn key_code(label: &str) -> String {
    let mut chars = label.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), None) if letter.is_ascii_alphabetic() => {
            format!("Key{}", letter.to_ascii_uppercase())
        }
        (Some(digit), None) if digit.is_ascii_digit() => format!("Digit{digit}"),
        _ => label.to_owned(),
    }
}

// Actual insertion of text

fn insert_text_into_focused_app(app: &AppHandle, text: String) -> Result<(), String> {
    app.clipboard()
        .write_text(text)
        .map_err(|error| format!("cannot write to the clipboard: {error}"))?;
    insert::send_paste().map_err(|error| error.to_string())
}

pub fn insert_current_text(app: &AppHandle) {
    let text = app.state::<LiveTextStore>().current();
    if text.trim().is_empty() {
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(message) = insert_text_into_focused_app(&app, text) {
            emit_ui_event(
                &app,
                UiEvent::Error {
                    session: None,
                    message,
                },
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_ctrl_shift_space() {
        assert_eq!(Hotkey::default_insert().label(), "Ctrl+Shift+Space");
    }

    #[test]
    fn letters_and_digits_get_short_labels() {
        assert_eq!(
            Hotkey::new(true, true, false, false, "KeyV")
                .unwrap()
                .label(),
            "Ctrl+Alt+V"
        );
        assert_eq!(
            Hotkey::new(false, false, true, true, "Digit7")
                .unwrap()
                .label(),
            "Shift+Meta+7"
        );
        assert_eq!(
            Hotkey::new(true, false, false, false, "F5")
                .unwrap()
                .label(),
            "Ctrl+F5"
        );
    }

    #[test]
    fn labels_parse_back_to_the_same_hotkey() {
        for hotkey in [
            Hotkey::default_insert(),
            Hotkey::new(true, true, true, true, "KeyZ").unwrap(),
            Hotkey::new(false, true, false, false, "Digit0").unwrap(),
            Hotkey::new(false, false, false, true, "ArrowUp").unwrap(),
        ] {
            assert_eq!(Hotkey::parse(&hotkey.label()), Ok(hotkey));
        }
    }

    #[test]
    fn a_key_without_modifier_is_rejected() {
        assert_eq!(
            Hotkey::new(false, false, false, false, "KeyV"),
            Err(HotkeyError::NoModifier)
        );
    }

    #[test]
    fn modifier_keys_alone_are_rejected() {
        for key in ["ControlLeft", "ShiftRight", "AltLeft", "MetaLeft"] {
            assert_eq!(
                Hotkey::new(true, false, false, false, key),
                Err(HotkeyError::OnlyModifiers(key.to_owned()))
            );
        }
    }

    #[test]
    fn an_empty_key_is_rejected() {
        assert_eq!(
            Hotkey::new(true, false, false, false, ""),
            Err(HotkeyError::NoKey)
        );
    }

    #[test]
    fn garbage_does_not_parse() {
        for text in ["", "+", "Ctrl+", "Hyper+KeyV", "V"] {
            assert!(Hotkey::parse(text).is_err(), "{text:?} should not parse");
        }
    }

    #[test]
    fn stored_text_with_stray_whitespace_still_parses() {
        assert_eq!(
            Hotkey::parse(" Ctrl + Shift + Space \n"),
            Ok(Hotkey::default_insert())
        );
    }
}
