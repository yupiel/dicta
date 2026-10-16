# Dictation

A small desktop dictation app built with Tauri 2 (Rust backend, Vue + TypeScript frontend). It records from the microphone, splits the audio at pauses, and transcribes everything locally with whisper.cpp. Windows is the target platform.

While you speak, each stretch of speech that ends in a pause (or reaches 10 seconds) is transcribed and shown as a **preview**. When you stop, the whole recording is transcribed once more and replaces the previews as the **final** text. While the model is busy, waiting previews are merged into one call (whisper pads every call to a 30 s window, so one longer call costs the same as a short one). Stopping abandons any previews still waiting; only the one the model is already working on has to finish before the final transcription starts.

With **auto mode** (Settings → Recording) recording continues after a long pause: the finished text stays on screen, and as soon as you speak again it is replaced by the new utterance.

Pressing **Ctrl+Shift+Space** (changeable in Settings) in any application pastes the text currently shown in the app into that application.

## Prerequisites (Windows)

- Rust 1.90 or newer (stable)
- Node.js and node package manager (pnpm was used during development)
- Visual Studio Build Tools with the C++ workload, CMake, and LLVM (libclang): whisper.cpp is compiled during the first build
- WebView2 runtime (included with Windows 11)

## Model

The easiest way is the **Download and load** button in the app. It fetches the model from Hugging Face into the app data directory (see below) and loads it. No API key is needed, and nothing else is ever sent over the network.

Alternatively, download a Whisper model in ggml format yourself, keep it outside the repository, and enter its path under "Use a different model file". Any ggml `.bin` model from whisper.cpp works, whatever its size, because the app converts the microphone audio to the 16 kHz mono format every Whisper model expects. Models in other formats (GGUF, faster-whisper/CTranslate2, PyTorch) do not work, and `.en` models only understand English:

- Current app default: [ggml-large-v3-turbo-q5_0.bin](https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin) (574 MB)

## Run

```
pnpm install
pnpm run tauri dev
```

In the app: open the sidebar (top left), go to **Settings** (bottom of the sidebar) to download or load a model and pick a microphone, then use the round button at the bottom:

| Button | Meaning |
| --- | --- |
| Select a model first (grey) | No model loaded; click it to open the settings |
| Record | Start; the window border turns red while recording and previews fade in |
| Stop | Border turns blue, text turns grey, button reads "Finalizing…" |
| New recording | Border flashes green, final text replaces the previews; clears the screen for the next take |

### Auto mode

Turn on **Auto mode** under Settings → Recording and choose the pause length in seconds (1 to 10, default 2). Press Record once; the app then:

1. records without live previews (they would queue ahead of the final transcription and delay it),
2. the moment speech has been followed by a pause of the chosen length, the border turns blue and only that utterance is transcribed, with most of the pause trimmed off; then the final text is shown, saved to the history, and the border turns green, while the microphone stays open,
3. waits. Only speech that is loud enough (RMS above 0.01, about -40 dBFS, well above the measured background noise) and lasts at least 0.3 s counts as a new utterance, so clicks, coughs and background noise leave the old text alone,
4. clears the old text when the new speech is confirmed, and shows the new utterance's text after its pause, then repeats from step 2.

If the model returns nothing for an utterance, the previous text is kept. Press Stop to end the session. Every utterance is its own history entry.

### Inserting into other applications

The shortcut works while the app runs, also in the background. It writes the current text to the clipboard and then sends Ctrl+V (Cmd+V on macOS) to the focused application. While recording that is the preview text so far, otherwise the final text; with auto mode it is the last finished utterance (nothing while a new one is being spoken). Notes:

- The clipboard keeps the text afterwards; the previous clipboard content is not restored.
- The default is `Ctrl+Shift+Space`. Change it under Settings → Insert into other apps: click the field and press the new keys (Esc cancels). It needs at least one modifier, takes effect at once and is saved as `insert_shortcut.txt` in the app data directory. If another program already owns the shortcut, Settings shows a message and the previous one stays active (at startup: the shortcut stays inactive until you pick another).
- Windows cannot send keystrokes into windows running as administrator unless this app does too.
- On macOS the app needs Accessibility permission (System Settings → Privacy & Security) to send the paste keystroke.
- Linux works on X11; Wayland generally blocks global shortcuts and synthetic input.

After a recording ends, a copy button appears next to the main button. Finished transcripts are saved automatically and listed under **History** in the sidebar, with date and time and a newest/oldest-first toggle.

The window has no native title bar; minimize, maximize and close are drawn in the app. Dragging the top strip moves the window, double-clicking it maximizes.

## Where data is stored

Everything lives in the per-user app data directory, which Tauri resolves per operating system (the folder is named after the identifier in `tauri.conf.json`):

| OS | Location |
| --- | --- |
| Windows | `%APPDATA%\net.yupiel.dicta` (Roaming) |
| Linux | `~/.local/share/net.yupiel.dicta` |
| macOS | `~/Library/Application Support/net.yupiel.dicta` |

Inside: `models/` (downloaded models), `history/` (one `<unix-ms>-<session>.txt` file per transcript, plain text). Audio is never written to disk.

## Security notes

- The window may only use what it needs: listen to backend events, drag/minimize/maximize/close itself, and write text to the clipboard (`src-tauri/capabilities/default.json`). Backend commands are the app's own and are not exposed to anything else.
- The global shortcut is registered by the Rust side only, so the window has no permission to register or unregister shortcuts.
- The web inspector is disabled (`devtools: false`) and the browser context menu is blocked; the only context menu is the one on history entries (show in folder, delete).
- On Linux the app sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` at startup to avoid a rendering failure on NVidia GPUs (tauri-apps/tauri#9394). Set `DICTA_HWACCEL` to any value to keep hardware rendering.

## Backend API

Commands (`invoke` from `src/api.ts`):

| Command | Arguments | Returns |
| --- | --- | --- |
| `list_input_devices` | none | `{ name, isDefault }[]` |
| `load_model` | `path` | nothing; takes a few seconds |
| `start_recording` | `options`: `deviceName`, `language`, `pauseMs`, `maxSegmentMs`, `autoPauseMs` (setting it turns on auto mode) (all optional) | session id |
| `stop_recording` | none | nothing; returns at once, the rest arrives as events |
| `get_status` | none | `{ phase, modelLoaded, insertShortcut, shortcutError }` |
| `set_insert_shortcut` | `hotkey`: `{ ctrl, alt, shift, meta, key }` (`key` is a `KeyboardEvent.code`) | the new shortcut's label, e.g. `Ctrl+Alt+V` |
| `reset_transcript` | none | nothing; forgets the text the shortcut would insert (the "New recording" button calls it) |
| `list_history` | none | `{ id, createdAtMs, text }[]`, newest first |
| `delete_history` | `id` | nothing |
| `reveal_history_entry` | `id` | nothing; opens the file manager at the transcript file |
| `clear_history` | none | nothing |
| `list_models` | none | `{ id, label, sizeMb, installed, path }[]` |
| `download_model` | `modelId` | path of the model file; progress arrives as events; one download at a time |

All progress arrives as one event named `dicta`, a union tagged by `type`:

| `type` | Fields | Meaning |
| --- | --- | --- |
| `phase` | `phase`: `idle`, `recording` or `finalizing` | Lifecycle changes |
| `modelLoaded` | `source` | A model is ready |
| `preview` | `session`, `index`, `text`, `durationMs`, `reason` | A segment was transcribed; `reason` is why it ended: `pause`, `maxLength` or `endOfRecording` |
| `utteranceStarted` | `session` | Auto mode only: speech was confirmed after a pause; clear the old text |
| `utteranceEnded` | `session` | Auto mode only: the pause was long enough, transcription of the utterance has begun |
| `final` | `session`, `text` | Transcription of the whole recording (auto mode: of one utterance, while recording continues) |
| `error` | `session`, `message` | Something failed; recording continues where possible |
| `historySaved` | `entry` | A final transcript was stored |
| `downloadProgress` | `modelId`, `downloaded`, `total` | Bytes received so far; `total` is null if the server sent no length |

## Layout

```
src/                  Vue + TypeScript frontend
  api.ts              Typed backend interface (commands and events)
  useDicta.ts         Reactive state and actions built on api.ts
  App.vue             Page layout
  components/         TitleBar, SideDrawer, HistoryPanel, SettingsPanel, ModelPanel, TranscriptView, RecordBar
  style.css           All styling (colours, window frame states)
src-tauri/src/
  commands.rs         Tauri commands (the only Tauri-specific code besides lib.rs)
  auto.rs             Auto mode: waits for confirmed speech between utterances
  live_text.rs        The text currently shown, kept for the insert shortcut
  hotkey.rs           The insert shortcut as data: validation, text form, label
  insert.rs           Pastes into the focused application (enigo)
  pipeline.rs         Audio chunks -> mono -> 16 kHz -> segments -> transcription
  segmenter.rs        Adaptive silence detection and segment cutting
  recorder.rs         Recording lifecycle: phases, sessions, events
  resample.rs         Downmix and band-limited resampling
  capture.rs          Microphone access through cpal
  stream.rs           StreamSource trait, so the engine is testable without hardware
  events.rs           Event handling for the frontend API
  whisper.rs          whisper-rs adapter
  history.rs          Transcript history as plain-text files
  models.rs           Model catalog and download (ureq, written to .part then renamed)
```

## AI usage

See [docs/AI_USAGE.md](docs/AI_USAGE.md).

## Tests

```
cd src-tauri
cargo test
```

The tests are unit tests and need no microphone or model.
