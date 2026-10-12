# AGENTS.md

## Project

A minimal dictation app: a **native desktop app** built with **Rust + Tauri 2** and a **TypeScript + Vue** UI.

Flow: record microphone audio → detect silence → transcribe **locally** (whisper.cpp or equivalent) → insert the text into the focused application via a global shortcut.

Code should be cross-platform compatible where possible. The main target platform is **Windows**.

## Hard constraints

- Audio capture and text insertion are implemented **natively in Rust at the system level**. No browser APIs (`getUserMedia`, Web Audio, web clipboard tricks) and no web-app architecture.
- The TypeScript frontend is a thin UI only: it shows state and sends user intent to Rust via Tauri commands/events. No business logic there.
- Transcription is fully local. No audio or text leaves the machine.

## Git: never commit

- **Do not run `git commit`, `git push`, or otherwise create commits.** The human commits.
- You may stage nothing and rewrite no history. When a logical unit of work is done, propose imperative-mood messages (with a short body when the "why" isn't obvious).

## Code quality

### Rust
- Clear ownership; avoid needless `clone()` and `Arc<Mutex<_>>` where plain ownership or channels work.
- Errors via `Result` and a meaningful error type (e.g. `thiserror`), propagated with `?`. No `unwrap()`/`expect()` outside tests unless an invariant is documented.
- No unnecessary `unsafe`. If unavoidable, keep it minimal behind a safe API with a `// SAFETY:` comment.
- Model app state with an enum (e.g. `Idle`, `Recording`, `Transcribing`, `Inserting`), not scattered booleans.
- Keep Tauri command handlers thin; put logic in testable modules.

### Audio pipeline
- Be explicit about sample rate, channels, and sample format. Convert to what the model expects (whisper.cpp: 16 kHz mono f32) in one place.
- Keep the real-time audio callback lean: no blocking, no heavy allocation. Hand samples to a processing thread via a bounded channel or ring buffer.
- Silence detection (e.g. RMS over a window plus a hangover/minimum duration) uses named, configurable parameters, not magic numbers.
- Keep pure audio logic (resampling, silence detection, buffering) separate from device I/O so it is unit-testable with synthetic signals.

### TypeScript 
- strict mode
- no unjustified `any`
- types matching the Tauri payloads

## Comments

Do not add comments where the meaning or purpose is clearly inferable from the code. Prefer good names and small functions. No narration (`// create the stream`) and no doc comments that restate a name.

Do comment the non-obvious: *why* a choice was made, OS/hardware quirks, model requirements (e.g. why 16 kHz), `// SAFETY:` justifications, known limitations.

## Verification

Before calling a task done, run what applies and make sure it passes:

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
- TypeScript type-check and lint
- For changes touching recording, silence detection, transcription, or insertion: run the app and exercise the behavior

Write unit tests for audio logic using synthetic signals (silence, sine waves, speech-then-silence). Never claim something works without running it; if something can't be verified in your environment (mic access, OS permissions), say so explicitly. Do not invent crate APIs, Tauri options, or whisper.cpp flags. Check the docs when unsure.

## AI usage

AI use must be visible and self-verified. Keep `docs/AI_USAGE.md` updated: where AI was used, for what, and how the output was checked. Review your own output critically (error paths, empty buffers, device disconnect, very short recordings, long silence) and understand every dependency and `unsafe` block you introduce.

## README

Must be understandable to a stranger: what the app does, platform and prerequisites, how to get the Whisper model, build/run steps, required OS permissions (microphone, accessibility), usage, a short architecture overview (capture → resample → silence detection → transcription → insertion), limitations, and a pointer to `docs/AI_USAGE.md`. Keep it in sync with the code.

## Working agreements

- Smallest change that solves the task; no unrelated refactors.
- Don't add model files, build artifacts, or secrets to the repo.
- When requirements are ambiguous, state your assumption or ask rather than guess silently.
