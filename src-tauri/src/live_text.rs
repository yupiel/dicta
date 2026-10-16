use std::sync::{Arc, Mutex, MutexGuard};

use crate::recorder::{Phase, RecorderEvent};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmenter::CutReason;

    fn preview(text: &str) -> RecorderEvent {
        RecorderEvent::Preview {
            session: 1,
            index: 0,
            text: text.to_owned(),
            duration_ms: 1000,
            reason: CutReason::Pause,
        }
    }

    fn final_text(text: &str) -> RecorderEvent {
        RecorderEvent::Final {
            session: 1,
            text: text.to_owned(),
        }
    }

    fn started() -> RecorderEvent {
        RecorderEvent::UtteranceStarted { session: 1 }
    }

    fn after(events: &[RecorderEvent]) -> String {
        let mut live = LiveText::default();
        for event in events {
            live.apply(event);
        }
        live.current()
    }

    #[test]
    fn previews_are_joined_while_recording() {
        let events = [
            RecorderEvent::Phase(Phase::Recording),
            preview(" hello"),
            preview("world "),
        ];
        assert_eq!(after(&events), "hello world");
    }

    #[test]
    fn the_final_text_replaces_the_previews() {
        let events = [preview("hello"), preview("wor"), final_text("hello world")];
        assert_eq!(after(&events), "hello world");
    }

    #[test]
    fn a_new_utterance_hides_the_old_text_at_once() {
        let events = [final_text("old"), started()];
        assert_eq!(after(&events), "");
    }

    #[test]
    fn a_new_utterance_replaces_the_old_text() {
        let events = [final_text("old"), started(), preview("new")];
        assert_eq!(after(&events), "new");
    }

    #[test]
    fn a_blank_result_for_the_new_utterance_brings_the_old_text_back() {
        let events = [final_text("old"), started(), final_text("  ")];
        assert_eq!(after(&events), "old");
    }

    #[test]
    fn a_blank_final_keeps_the_earlier_text() {
        let events = [
            final_text("old"),
            started(),
            preview("new"),
            final_text(" "),
        ];
        assert_eq!(after(&events), "old");
    }

    #[test]
    fn starting_a_recording_clears_everything() {
        let events = [final_text("old"), RecorderEvent::Phase(Phase::Recording)];
        assert_eq!(after(&events), "");
    }

    #[test]
    fn nothing_is_inserted_before_anything_was_said() {
        assert_eq!(LiveText::default().current(), "");
    }

    #[test]
    fn clearing_forgets_the_text() {
        let store = LiveTextStore::default();
        store.apply(&final_text("hello"));
        store.clear();
        assert_eq!(store.current(), "");
    }
}
