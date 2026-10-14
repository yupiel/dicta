import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { DictaEvent, downloadModel, listModels, loadModel, onDictaEvent, UiState, type DownloadState, type ModelInfo } from '../api';

export function useDicta() {
    const error = ref<string>('');

    // model handling
    const models = ref<ModelInfo[]>([]);
    const download = ref<DownloadState | null>(null);
    const loadedSource = ref<string | null>(null);
    const loadingModel = ref<boolean>(false);
    const modelLoaded = ref<boolean>(false);

    // TODO: transcription handling
    const transcribing = ref<boolean>(false);
    const finalText = ref<string | null>(null);
    let lastFinal: string | null = null;

    const uiState = computed<UiState>(() => {
        if (finalText.value !== null) return 'done';
        return modelLoaded.value ? 'idle' : 'noModel';
    });

    function handleDictaEvent(event: DictaEvent) {
        switch (event.type) {
            case 'downloadProgress':
                if (download.value) {
                    download.value = { modelId: event.modelId, downloaded: event.downloaded, total: event.total };
                }
                break;
            case 'error':
                error.value = event.message;
                break;
            case 'final':
                transcribing.value = false;
                if (!event.text.trim() && lastFinal !== null) {
                    finalText.value = lastFinal;
                } else {
                    finalText.value = event.text;
                    if (event.text.trim()) lastFinal = event.text;
                }
                break;
            case 'modelLoaded':
                modelLoaded.value = true;
                loadedSource.value = event.source;
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

    let stopListen: (() => void) | undefined;
    let unmounted = false;

    onMounted(() =>
        run(async () => {
            const stopListening = await onDictaEvent(handleDictaEvent);
            if (unmounted) {
                stopListening();
                return;
            }
            stopListen = stopListening;

            models.value = await listModels();
        }),
    );

    onBeforeUnmount(() => {
        unmounted = true;
        stopListen?.();
    });

    return {
        error,
        uiState,

        // model handling
        models,
        download,
        loadedSource,
        loadingModel,
        modelLoaded,
        load,
        downloadAndLoad,

        // TODO: transcription
        transcribing,
        finalText,
    };
}