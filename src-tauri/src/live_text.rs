use std::sync::{Arc, Mutex, MutexGuard};

use crate::recorder::{RecorderEvent, Phase};

/// What the user currently sees as the transcript, so it can be inserted elsewhere on demand.
#[derive(Debug, Default)]
pub struct LiveText {
    previews: Vec<String>,
    confirmed: Option<String>,
    replace_pending: bool,
}

impl LiveText {
    pub fn apply(&mut self, event: &RecorderEvent) {
        match event {
            RecorderEvent::Phase(Phase::Recording) => *self = Self::default(),
            RecorderEvent::UtteranceStarted { .. } => self.replace_pending = true,
            RecorderEvent::Preview { text, .. } if !text.trim().is_empty() => {
                if self.replace_pending {
                    self.previews.clear();
                    self.replace_pending = false;
                }
                self.previews.push(text.trim().to_owned());
            }
            RecorderEvent::Final { text, .. } => {
                self.previews.clear();
                self.replace_pending = false;
                // A blank result (noise Whisper could not read) must not wipe out earlier text.
                if !text.trim().is_empty() || self.confirmed.is_none() {
                    self.confirmed = Some(text.trim().to_owned());
                }
            }
            _ => {}
        }
    }

    pub fn current(&self) -> String {
        if self.previews.is_empty() {
            if self.replace_pending {
                return String::new();
            }
            self.confirmed.clone().unwrap_or_default()
        } else {
            self.previews.join(" ")
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Default)]
pub struct LiveTextStore(Arc<Mutex<LiveText>>);

impl LiveTextStore {
    // Plain data, so a panic elsewhere must not make the shortcut stop working.
    fn lock(&self) -> MutexGuard<'_, LiveText> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn apply(&self, event: &RecorderEvent) {
        self.lock().apply(event);
    }

    pub fn current(&self) -> String {
        self.lock().current()
    }

    pub fn clear(&self) {
        self.lock().clear();
    }
}