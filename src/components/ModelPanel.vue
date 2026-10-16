<script setup lang="ts">
import { computed, ref } from 'vue';
import type { ModelInfo } from '../api';
import type { DownloadState } from '../composables/useDicta';

const props = defineProps<{
  models: ModelInfo[];
  download: DownloadState | null;
  canLoad: boolean;
  loadedSource: string | null;
  loading: boolean;
}>();

const emit = defineEmits<{
  load: [path: string];
  download: [modelId: string];
}>();

const customPath = ref('');

const downloading = computed(() => props.download !== null);
const percent = computed(() => {
  const state = props.download;
  if (!state?.total) return null;
  return Math.min(100, Math.round((state.downloaded / state.total) * 100));
});
</script>

<template>
  <section class="field-group">
    <h3>Model</h3>
    <ul class="model-list">
      <li
        v-for="model in models"
        :key="model.id"
        class="model"
      >
        <div class="model__info">
          <span class="model__name">{{ model.label }}</span>
          <span class="muted">{{ model.sizeMb }} MB</span>
        </div>
        <span
          v-if="model.path === loadedSource"
          class="badge"
        >Loaded</span>
        <button
          v-else-if="model.installed"
          class="btn"
          type="button"
          :disabled="!canLoad"
          @click="emit('load', model.path)"
        >
          {{ loading ? 'Loading…' : 'Load' }}
        </button>
        <button
          v-else
          class="btn btn--primary"
          type="button"
          :disabled="!canLoad || downloading"
          @click="emit('download', model.id)"
        >
          Download
        </button>
        <progress
          v-if="download?.modelId === model.id"
          class="model__progress"
          :value="percent ?? undefined"
          max="100"
          :aria-label="`Downloading ${model.label}`"
        />
      </li>
    </ul>

    <details class="custom-model">
      <summary>Use a different model file</summary>
      <label
        class="field"
        for="model-path"
      >
        <span>Path to a Whisper model in ggml format (.bin)</span>
        <div class="field__row">
          <input
            id="model-path"
            v-model="customPath"
            class="input"
            type="text"
            placeholder="C:\models\ggml-base.bin"
          >
          <button
            class="btn"
            type="button"
            :disabled="!canLoad || !customPath.trim()"
            @click="emit('load', customPath.trim())"
          >
            Load
          </button>
        </div>
      </label>
    </details>
  </section>
</template>
