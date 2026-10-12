# Dictation

A small desktop dictation app built with Tauri 2 (Rust backend, Vue + TypeScript frontend). It records from the microphone, splits the audio at pauses, and transcribes everything locally with whisper.cpp. Windows is the target platform.

## Prerequisites (Windows)

- Rust (stable)
- Node.js and node package manager (pnpm was used during development)
- WebView2 runtime (included with Windows 11)

## Run

```
pnpm install
pnpm run tauri dev
```

## AI usage

See [docs/AI_USAGE.md](docs/AI_USAGE.md).
