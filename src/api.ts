import { invoke } from '@tauri-apps/api/core';

// --- Types ---
export interface InputDevice {
    name: string;
    id: string;
    isDefault: boolean;
}

export interface StartOptions {
    deviceId: string | undefined;
}

// --- Tauri Commands ---

export const listInputDevices = () => invoke<InputDevice[]>('list_input_devices');
export const startRecording = (options: StartOptions) => invoke('start_recording', { options });