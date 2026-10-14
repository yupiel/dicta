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

## Where the AI was wrong
- A boxed error did not convert with `?`; fixed with an explicit `map_err`.
- Attempted to pull more packages than necessary for eslint: `@eslint/js`, `globals`

## How I verify

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `pnpm lint:fix` (including fixing errors that can't be fixed automatically)
- `pnpm run build` (runs `vue-tsc --noEmit`, then the Vite build)
- Manual runs on Windows, then listening to the saved segments
