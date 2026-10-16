# AI usage

## Tool

Claude (Anthropic) as a coding assistant. Model: Sonnet 5.5.

## Rules

- The AI never commits. I review every change and commit myself, with an `Assisted-by:` trailer on commits that contain AI-written code.
- Nothing counts as working until it has been built and run.
- Claims about crate and Tauri APIs are checked against the documentation.
- No audio recordings, model files, or secrets are shared with the AI.

## Where AI was used

| Area | AI contribution | How I verified it |
| --- | --- | --- |
| Microphone capture (cpal) | OS specific new crate version's input capture | ran on my Windows machine with my microphone; tried unplugging the device |
| Eslint | Setup including Vue and Typescript checks | ran lints and lint fixes by hand and appended rules important to me
| Frontend (Vue + TypeScript) | Drafted the components and the styling | `vue-tsc` type check; static HTML mock of the CSS states rendered in Chromium; clicked through the real UI in the Tauri window |
| Recorder | Drafted the phase state machine |  |
| Whisper adapter | Drafted the whisper-rs wrapper | checked parameters against the whisper-rs docs and issues |
| segmenter | Drafted the silence detection | |
| model download | Outlined `models.rs` and the matching commands and Vue components | downloaded a model on my machine |
| History store | Drafted `history.rs` and the matching commands and Vue components | checked the history files on my machine |
| Auto mode | Drafted the pause timer in the segmenter, the `Waiter` that confirms speech between utterances, the pipeline stages and the replace logic in the frontend | Unit tests with synthetic signals (speech, long pause, click, quiet speech, second utterance, stop while waiting); tried it with my voice and different pause lengths |
| Unit tests | generated unit tests for various components which did not have unit tests but might have complex functionality | manually checked the tests to make sure they make sense and don't test against unintended functionality
| README | rough outline of the README file for the user to fill out | 

## Where the AI was wrong
- A boxed error did not convert with `?`; fixed with an explicit `map_err`.
- Attempted to pull more packages than necessary for eslint: `@eslint/js`, `globals`
- A fixed RMS silence threshold dropped quiet speech and cut segments too early. Replaced by an adaptive noise-floor threshold with hysteresis.
- It assumed whisper-rs ignores earlier text between calls. `no_context` defaults to false, so previous segments leaked into later ones; it is now set explicitly.
- The first resampler decimated without a low-pass filter, which aliased speech and made previews unintelligible. Replaced by a windowed-sinc resampler.
- Two auto-mode tests had wrong expectations (a lead-in bound off by one chunk, and the length of the second utterance, which includes up to 1.5 s of kept lead-in). The tests were wrong, not the code.
- A test expected the wrong event sequence (missing the model-loaded event). The test was wrong, not the code.

## How I verify

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `pnpm lint:fix` (including fixing errors that can't be fixed automatically)
- `pnpm run build` (runs `vue-tsc --noEmit`, then the Vite build)
- Manual runs on Windows, then listening to the saved segments
