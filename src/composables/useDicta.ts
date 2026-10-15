import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import {
  clearHistory,
  deleteHistory,
  downloadModel,
  getStatus,
  listHistory,
  listInputDevices,
  listModels,
  loadModel,
  onDictaEvent,
  resetTranscript,
  revealHistoryEntry,
  startRecording,
  stopRecording,
  type UiState,
  type DictaEvent,
  type HistoryEntry,
  type InputDevice,
  type ModelInfo,
  type Phase,
} from '../api';

export interface AutoMode {
  enabled: boolean;
  pauseSeconds: number;
}

export interface DownloadState {
  modelId: string;
  downloaded: number;
  total: number | null;
}

const COPIED_FEEDBACK_MS = 1500;
const MIN_AUTO_PAUSE_SECONDS = 1;
const MAX_AUTO_PAUSE_SECONDS = 10;
const FALLBACK_AUTO_PAUSE_SECONDS = 2;

function autoPauseMs(auto: AutoMode): number | undefined {
  if (!auto.enabled) return undefined;
  const seconds = Number(auto.pauseSeconds) || FALLBACK_AUTO_PAUSE_SECONDS;
  return Math.round(Math.min(MAX_AUTO_PAUSE_SECONDS, Math.max(MIN_AUTO_PAUSE_SECONDS, seconds)) * 1000);
}

export function useDicta() {
  const phase = ref<Phase>('idle');
  const modelLoaded = ref<boolean>(false);
  const loadedSource = ref<string | null>(null);
  const loadingModel = ref<boolean>(false);
  const devices = ref<InputDevice[]>([]);
  const previews = ref<string[]>([]);
  const finalText = ref<string | null>(null);
  const error = ref<string>('');
  const history = ref<HistoryEntry[]>([]);
  const models = ref<ModelInfo[]>([]);
  const download = ref<DownloadState | null>(null);
  const copied = ref<boolean>(false);
  let lastFinal: string | null = null;
  const transcribing = ref<boolean>(false);

  const uiState = computed<UiState>(() => {
    if (transcribing.value && phase.value === 'recording') return 'finalizing';
    if (finalText.value !== null && phase.value === 'recording') return 'listening';
    if (finalText.value !== null) return 'done';
    if (phase.value === 'recording') return 'recording';
    if (phase.value === 'finalizing') return 'finalizing';
    return modelLoaded.value ? 'idle' : 'noModel';
  });
  const canLoad = computed(() => phase.value === 'idle' && !loadingModel.value);
  const canCopy = computed(() => !!finalText.value);

  function handle(event: DictaEvent) {
    switch (event.type) {
      case 'phase':
        phase.value = event.phase;
        if (event.phase === 'recording') {
          previews.value = [];
          finalText.value = null;
          error.value = '';
          lastFinal = null;
          transcribing.value = false;
        }
        break;
      case 'modelLoaded':
        modelLoaded.value = true;
        loadedSource.value = event.source;
        break;
      case 'utteranceStarted':
        previews.value = [];
        finalText.value = null;
        break;
      case 'utteranceEnded':
        transcribing.value = true;
        break;
      case 'preview':
        if (event.text.trim()) previews.value.push(event.text);
        break;
      case 'final':
        transcribing.value = false;
        if (!event.text.trim() && lastFinal !== null) {
          finalText.value = lastFinal;
        } else {
          finalText.value = event.text;
          if (event.text.trim()) lastFinal = event.text;
        }
        previews.value = [];
        break;
      case 'error':
        error.value = event.message;
        break;
      case 'historySaved':
        history.value.unshift(event.entry);
        break;
      case 'downloadProgress':
        if (download.value) {
          download.value = { modelId: event.modelId, downloaded: event.downloaded, total: event.total };
        }
        break;
    }
  }

  async function run(action: () => Promise<unknown>) {
    try {
      await action();
    } catch (cause) {
      error.value = String(cause);
    }
  }

  const load = (path: string) =>
    run(async () => {
      loadingModel.value = true;
      try {
        await loadModel(path.trim());
      } finally {
        loadingModel.value = false;
      }
    });

  const downloadAndLoad = (modelId: string) =>
    run(async () => {
      download.value = { modelId, downloaded: 0, total: null };
      try {
        const path = await downloadModel(modelId);
        models.value = await listModels();
        download.value = null;
        loadingModel.value = true;
        await loadModel(path);
      } finally {
        download.value = null;
        loadingModel.value = false;
      }
    });

  const removeHistoryEntry = (id: string) =>
    run(async () => {
      await deleteHistory(id);
      history.value = history.value.filter((entry) => entry.id !== id);
    });

  const revealEntry = (id: string) => run(() => revealHistoryEntry(id));

  const clearAllHistory = () =>
    run(async () => {
      await clearHistory();
      history.value = [];
    });

  const start = (deviceId: string | undefined, language: string, auto: AutoMode) =>
    run(() =>
      startRecording({
        deviceId: deviceId || undefined,
        language: language.trim() || undefined,
        autoPauseMs: autoPauseMs(auto),
      }),
    );

  const stop = () => run(stopRecording);

  function reset() {
    previews.value = [];
    finalText.value = null;
    error.value = '';
    copied.value = false;
    lastFinal = null;
    void run(resetTranscript);
  }

  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  const copyTranscript = () =>
    run(async () => {
      if (!finalText.value) return;
      copied.value = true;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied.value = false), COPIED_FEEDBACK_MS);
    });

  let stopListen: (() => void) | undefined;
  let unmounted = false;

  onMounted(() =>
    run(async () => {
      const stopListening = await onDictaEvent(handle);
      if (unmounted) {
        stopListening();
        return;
      }
      stopListen = stopListening;

      devices.value = await listInputDevices();
      history.value = await listHistory();
      models.value = await listModels();
      const status = await getStatus();
      phase.value = status.phase;
      modelLoaded.value = status.modelLoaded;
    }),
  );

  onBeforeUnmount(() => {
    unmounted = true;
    clearTimeout(copiedTimer);
    stopListen?.();
  });

  return {
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
  };
}
