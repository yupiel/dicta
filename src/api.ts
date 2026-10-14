import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

// --- Types ---
export interface InputDevice {
    name: string;
    id: string;
    isDefault: boolean;
}

export interface StartOptions {
    deviceId: string | undefined;
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

export type UiState = 'noModel' | 'idle' | 'recording' | 'finalizing' | 'done';

export type DictaEvent =
    | { type: 'modelLoaded'; source: string }
    | { type: 'final'; session: number; text: string }
    | { type: 'error'; session: number | null; message: string }
    | { type: 'downloadProgress'; modelId: string; downloaded: number; total: number | null };

// --- Tauri Commands ---
export const listInputDevices = () => invoke<InputDevice[]>('list_input_devices');
export const startRecording = (options: StartOptions) => invoke('start_recording', { options });
export const stopRecording = () => invoke<void>('stop_recording');

export const listModels = () => invoke<ModelInfo[]>('list_models');
export const downloadModel = (modelId: string) => invoke<string>('download_model', { modelId });
export const loadModel = (path: string) => invoke<void>('load_model', { path });

export const onDictaEvent = (handler: (event: DictaEvent) => void,
): Promise<UnlistenFn> => listen<DictaEvent>('dicta', (event) => handler(event.payload));

// Window Controls
export const minimizeWindow = () => getCurrentWindow().minimize();
export const toggleMaximizeWindow = () => getCurrentWindow().toggleMaximize();
export const closeWindow = () => getCurrentWindow().close();
