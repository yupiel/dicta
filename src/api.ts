import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';

// --- Types ---
export interface InputDevice {
    name: string;
    id: string;
    isDefault: boolean;
}

export interface StartOptions {
    deviceId?: string;
    language?: string;
    pauseMs?: number;
    maxSegmentMs?: number;
    autoPauseMs?: number;
}

export interface ModelInfo {
    id: string;
    label: string;
    sizeMb: number;
    installed: boolean;
    path: string;
}

export interface DownloadState {
    modelId: string;
    downloaded: number;
    total: number | null;
}

export type UiState = 'noModel' | 'idle' | 'recording' | 'listening' | 'finalizing' | 'done';

export type DictaEvent =
    | { type: 'modelLoaded'; source: string }
    | { type: 'final'; session: number; text: string }
    | { type: 'error'; session: number | null; message: string }
    | { type: 'downloadProgress'; modelId: string; downloaded: number; total: number | null }
    | { type: 'phase'; phase: Phase }
    | {
        type: 'preview';
        session: number;
        index: number;
        text: string;
        durationMs: number;
        reason: CutReason;
    }
    | { type: 'utteranceStarted'; session: number }
    | { type: 'utteranceEnded'; session: number }
    | { type: 'historySaved'; entry: HistoryEntry }
    | { type: 'downloadProgress'; modelId: string; downloaded: number; total: number | null };

export interface Status {
    phase: Phase;
    modelLoaded: boolean;
    insertShortcut: string;
    shortcutError: string | null;
}

export interface HistoryEntry {
    id: string;
    createdAtMs: number;
    text: string;
}

export type Phase = 'idle' | 'recording' | 'finalizing';
export type CutReason = 'pause' | 'maxLength' | 'endOfRecording';

export interface Hotkey {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
  key: string;
}

// --- Tauri Commands ---
export const listInputDevices = () => invoke<InputDevice[]>('list_input_devices');
export const startRecording = (options: StartOptions) => invoke('start_recording', { options });
export const stopRecording = () => invoke<void>('stop_recording');

export const listModels = () => invoke<ModelInfo[]>('list_models');
export const downloadModel = (modelId: string) => invoke<string>('download_model', { modelId });
export const loadModel = (path: string) => invoke<void>('load_model', { path });

export const getStatus = () => invoke<Status>('get_status');
export const resetTranscript = () => invoke<void>('reset_transcript');

export const listHistory = () => invoke<HistoryEntry[]>('list_history');
export const deleteHistory = (id: string) => invoke<void>('delete_history', { id });
export const revealHistoryEntry = (id: string) => invoke<void>('reveal_history_entry', { id });
export const clearHistory = () => invoke<void>('clear_history');

export const onDictaEvent = (handler: (event: DictaEvent) => void,
): Promise<UnlistenFn> => listen<DictaEvent>('dicta', (event) => handler(event.payload));

export const copyText = (text: string) => writeText(text);
export const setInsertShortcut = (hotkey: Hotkey) => invoke<string>('set_insert_shortcut', { hotkey });

// Window Controls
export const minimizeWindow = () => getCurrentWindow().minimize();
export const toggleMaximizeWindow = () => getCurrentWindow().toggleMaximize();
export const closeWindow = () => getCurrentWindow().close();
