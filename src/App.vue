<script setup lang="ts">
import { ref, watch } from 'vue';
import HistoryPanel from './components/HistoryPanel.vue';
import RecordBar from './components/RecordBar.vue';
import SettingsPanel from './components/SettingsPanel.vue';
import SideDrawer from './components/SideDrawer.vue';
import TitleBar from './components/TitleBar.vue';
import TranscriptView from './components/TranscriptView.vue';
import { AutoMode, useDicta } from './composables/useDicta.ts';

const {
  devices,
  error,
  previews,
  finalText,
  uiState,
  loadedSource,
  loadingModel,
  copied,
  canLoad,
  canCopy,
  history,
  models,
  download,
  downloadAndLoad,
  removeHistoryEntry,
  revealEntry,
  clearAllHistory,
  load,
  start,
  stop,
  reset,
  copyTranscript,
} = useDicta();

const selectedInputDevice = ref<string | undefined>(undefined);
const sidebarOpen = ref<boolean>(false);
const sidebarView = ref<'history' | 'settings'>('history');
const language = ref('de');

const AUTO_STORAGE_KEY = 'dictation.autoMode';
const DEFAULT_AUTO_PAUSE_SECONDS = 2;

function loadAutoMode(): AutoMode {
  const fallback = { enabled: false, pauseSeconds: DEFAULT_AUTO_PAUSE_SECONDS };
  try {
    const stored = JSON.parse(localStorage.getItem(AUTO_STORAGE_KEY) ?? 'null');
    if (typeof stored?.enabled === 'boolean' && Number.isFinite(stored?.pauseSeconds)) {
      return { enabled: stored.enabled, pauseSeconds: stored.pauseSeconds };
    }
  } catch {
    // Unreadable storage only means the defaults apply.
  }
  return fallback;
}

const autoMode = ref<AutoMode>(loadAutoMode());
  
watch(
  autoMode,
  (value) => {
    try {
      localStorage.setItem(AUTO_STORAGE_KEY, JSON.stringify(value));
    } catch {
      // Not persisting is acceptable; the setting still applies to this run.
      // TODO: Find a way to inform the user that this happened
    }
  },
  { deep: true },
);

function openSettings() {
  sidebarView.value = 'settings';
  sidebarOpen.value = true;
}
</script>

<template>
  <div
    class="app"
    :data-state="uiState"
  >
    <TitleBar
      :sidebar-open="sidebarOpen"
      @toggle-sidebar="sidebarOpen = !sidebarOpen"
    />

    <main class="stage">
      <TranscriptView
        :state="uiState"
        :previews="previews"
        :final-text="finalText"
        :loading-model="loadingModel"
      />
      <p
        v-if="error"
        class="notice"
        role="alert"
      >
        {{ error }}
      </p>
    </main>

    <RecordBar
      :state="uiState"
      :loading-model="loadingModel"
      :copied="copied"
      :can-copy="canCopy"
      @record="start(selectedInputDevice, language, autoMode)"
      @stop="stop()"
      @reset="reset()"
      @copy="copyTranscript()"
      @need-model="openSettings()"
    />

    <SideDrawer
      v-model:open="sidebarOpen"
      v-model:view="sidebarView"
    >
      <template #history>
        <HistoryPanel
          :entries="history"
          @remove="removeHistoryEntry"
          @reveal="revealEntry"
          @clear="clearAllHistory"
        />
      </template>
      <template #settings>
        <SettingsPanel
          v-model:selected-input-device="selectedInputDevice"
          v-model:language="language"
          v-model:auto-mode="autoMode"
          :devices="devices"
          :models="models"
          :download="download"
          :can-load="canLoad"
          :loaded-source="loadedSource"
          :loading="loadingModel"
          @load="load"
          @download="downloadAndLoad"
        />
      </template>
    </SideDrawer>
  </div>
</template>
