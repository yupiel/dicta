<script setup lang="ts">
import type { DownloadState, InputDevice, ModelInfo } from '../api';
import ModelPanel from './ModelPanel.vue';

const selectedInputDevice = defineModel<string | undefined>('selectedInputDevice', { required: true });

defineProps<{
    devices: InputDevice[];
    models: ModelInfo[];
    download: DownloadState | null;
    loadedSource: string | null;
    loading: boolean;
}>();

const emit = defineEmits<{
    load: [path: string];
    download: [modelId: string];
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
            :key="device.id"
            :value="device.id"
          >
            {{ device.name }}{{ device.isDefault ? ' (default)' : '' }}
          </option>
        </select>
      </label>
    </section>

    <ModelPanel
      :models="models"
      :download="download"
      :loaded-source="loadedSource"
      :loading="loading"
      @load="emit('load', $event)"
      @download="emit('download', $event)"
    />
  </section>
</template>
