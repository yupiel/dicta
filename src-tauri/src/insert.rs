use std::thread;
use std::time::Duration;

use enigo::{Direction, Enigo, InputError, Key, Keyboard, NewConError, Settings};

// Without a pause the target app can still see the shortcut's own modifiers as held.
const MODIFIER_RELEASE_SETTLE: Duration = Duration::from_millis(40);

#[derive(Debug, thiserror::Error)]
pub enum InsertError {
    #[error("cannot reach the input system: {0}")]
    Connect(#[from] NewConError),
    #[error("cannot send the paste keystroke: {0}")]
    Send(#[from] InputError),
}

/// Presses the paste shortcut in whichever application has focus; the text must already be on
/// the clipboard. Typing the text directly would depend on the keyboard layout and be slow for
/// long dictations.
pub fn send_paste() -> Result<(), InsertError> {
    let mut enigo = Enigo::new(&Settings::default())?;

    // The global shortcut fires while its modifiers are still down, and a held Shift or Alt
    // would turn the paste into a different command in the target app.
    for modifier in [Key::Control, Key::Alt, Key::Shift] {
        enigo.key(modifier, Direction::Release)?;
    }
    thread::sleep(MODIFIER_RELEASE_SETTLE);

    let paste_modifier = if cfg!(target_os = "macos") {
        Key::Meta
    } else {
        Key::Control
    };
    
    enigo.key(paste_modifier, Direction::Press)?;
    let pasted = enigo.key(Key::Unicode('v'), Direction::Click);
    enigo.key(paste_modifier, Direction::Release)?;
    pasted?;

    Ok(())
}
