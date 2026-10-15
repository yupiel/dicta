<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import type { UiState } from '../api';

const props = defineProps<{
  state: UiState;
  previews: string[];
  finalText: string | null;
  loadingModel: boolean;
}>();

const scroller = ref<HTMLElement | null>(null);

const tone = computed(() => (props.state === 'finalizing' ? 'pending' : 'normal'));

const placeholder = computed(() => {
  switch (props.state) {
    case 'noModel':
      return props.loadingModel
        ? 'Loading the model…'
        : 'Open the sidebar, then Settings, to download or select a model.';
    case 'recording':
    case 'listening':
      return 'Listening…';
    case 'finalizing':
      return 'Transcribing…';
    default:
      return 'Press record and start speaking.';
  }
});

watch(
  () => props.finalText,
  () => scroller.value?.scrollTo({ top: 0 }),
);

watch(
  () => props.previews.length,
  async () => {
    await nextTick();
    scroller.value?.scrollTo({ top: scroller.value.scrollHeight, behavior: 'smooth' });
  },
);
</script>

<template>
  <section
    ref="scroller"
    class="transcript"
    :data-tone="tone"
    aria-live="polite"
  >
    <p
      v-if="finalText"
      class="transcript__final"
    >
      {{ finalText }}
    </p>
    <p
      v-else-if="finalText === ''"
      class="transcript__placeholder"
    >
      No speech detected.
    </p>
    <TransitionGroup
      v-else-if="previews.length"
      tag="p"
      name="segment"
      class="transcript__live"
    >
      <span
        v-for="(text, index) in previews"
        :key="index"
        class="segment"
      >{{ text }}</span>
    </TransitionGroup>
    <p
      v-else
      class="transcript__placeholder"
    >
      {{ placeholder }}
    </p>
  </section>
</template>
