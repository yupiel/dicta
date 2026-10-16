<script setup lang="ts">
import type { Hotkey, InputDevice, ModelInfo } from '../api';
import type { AutoMode, DownloadState } from '../composables/useDicta';
import HotkeyInput from './HotkeyInput.vue';
import ModelPanel from './ModelPanel.vue';

const selectedInputDevice = defineModel<string | undefined>('selectedInputDevice', { required: true });
const language = defineModel<string>('language', { required: true });
const autoMode = defineModel<AutoMode>('autoMode', { required: true });

defineProps<{
  devices: InputDevice[];
  models: ModelInfo[];
  download: DownloadState | null;
  canLoad: boolean;
  loadedSource: string | null;
  loading: boolean;
  insertShortcut: string;
  shortcutError: string | null;
}>();

const emit = defineEmits<{
  load: [path: string];
  download: [modelId: string];
  changeShortcut: [hotkey: Hotkey];
}>();
</script>

<template>
  <section>
    <div class="panel-header">
      <h2>Settings</h2>
    </div>

    <section class="field-group">
      <h3>Audio</h3>
      <label
        class="field"
        for="device"
      >
        <span>Microphone</span>
        <select
          id="device"
          v-model="selectedInputDevice"
          class="input"
        >
          <option
            v-for="device in devices"
            :key="device.name"
            :value="device.id"
          >
            {{ device.name }}{{ device.isDefault ? ' (default)' : '' }}
          </option>
        </select>
      </label>
      <label
        class="field"
        for="language"
      >
        <span>Language code (empty = auto-detect)</span>
        <input
          id="language"
          v-model="language"
          class="input"
          type="text"
        >
      </label>
    </section>

    <section class="field-group">
      <h3>Recording</h3>
      <label
        class="field field--inline"
        for="auto-mode"
      >
        <span>Auto mode</span>
        <input
          id="auto-mode"
          v-model="autoMode.enabled"
          type="checkbox"
          role="switch"
          class="toggle"
        >
      </label>
      <p class="muted hint">
        Keeps recording after a long pause. The text stays until you speak again, then it is replaced.
      </p>
      <label
        class="field"
        for="auto-pause"
      >
        <span>Pause that ends an utterance (seconds)</span>
        <input
          id="auto-pause"
          v-model.number="autoMode.pauseSeconds"
          class="input"
          type="number"
          min="1"
          max="10"
          step="0.5"
          :disabled="!autoMode.enabled"
        >
      </label>
    </section>

    <section class="field-group">
      <h3>Insert into other apps</h3>
      <p class="muted hint">
        Press <kbd>{{ insertShortcut }}</kbd> in any application to paste the text currently shown here.
      </p>
      <label
        class="field"
        for="insert-shortcut"
      >
        <span>Shortcut (click, then press the new keys)</span>
        <HotkeyInput
          id="insert-shortcut"
          :label="insertShortcut"
          @change="emit('changeShortcut', $event)"
        />
      </label>
      <p
        v-if="shortcutError"
        class="field-error"
        role="alert"
      >
        {{ shortcutError }}
      </p>
    </section>

    <ModelPanel
      :models="models"
      :download="download"
      :can-load="canLoad"
      :loaded-source="loadedSource"
      :loading="loading"
      @load="emit('load', $event)"
      @download="emit('download', $event)"
    />
  </section>
</template>
