<script setup lang="ts">
import { computed } from 'vue';
import type { UiState } from '../api';

const props = defineProps<{
  state: UiState;
  loadingModel: boolean;
  copied: boolean;
  canCopy: boolean;
}>();

const emit = defineEmits<{
  record: [];
  stop: [];
  reset: [];
  copy: [];
  needModel: [];
}>();

const label = computed(() => {
  switch (props.state) {
    case 'noModel':
      return props.loadingModel ? 'Loading model…' : 'Select a model first';
    case 'idle':
      return 'Record';
    case 'recording':
    case 'listening':
      return 'Stop';
    case 'finalizing':
      return 'Finalizing…';
    case 'done':
      return 'New recording';
  }
});

const inactive = computed(() => props.state === 'noModel' || props.state === 'finalizing');

function press() {
  switch (props.state) {
    case 'noModel':
      if (!props.loadingModel) emit('needModel');
      break;
    case 'idle':
      emit('record');
      break;
    case 'recording':
    case 'listening':
      emit('stop');
      break;
    case 'done':
      emit('reset');
      break;
    case 'finalizing':
      break;
  }
}
</script>

<template>
  <footer class="recordbar">
    <button
      class="record-button"
      type="button"
      :data-state="state"
      :aria-disabled="inactive"
      @click="press"
    >
      <svg
        v-if="state === 'idle' || state === 'noModel'"
        viewBox="0 0 24 24"
        width="20"
        height="20"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <rect
          x="9"
          y="3"
          width="6"
          height="11"
          rx="3"
        />
        <path d="M5 11a7 7 0 0 0 14 0M12 18v3" />
      </svg>
      <span
        v-else-if="state === 'recording' || state === 'listening'"
        class="record-dot"
        aria-hidden="true"
      />
      <span
        v-else-if="state === 'finalizing'"
        class="spinner"
        aria-hidden="true"
      />
      <svg
        v-else
        viewBox="0 0 24 24"
        width="20"
        height="20"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        aria-hidden="true"
      >
        <path d="M12 5v14M5 12h14" />
      </svg>
      {{ label }}
    </button>

    <Transition name="pop">
      <button
        v-if="state === 'done' || state === 'listening'"
        class="round-button"
        type="button"
        :disabled="!canCopy"
        :aria-label="copied ? 'Copied' : 'Copy transcript'"
        :title="copied ? 'Copied' : 'Copy transcript'"
        @click="emit('copy')"
      >
        <svg
          v-if="copied"
          viewBox="0 0 24 24"
          width="20"
          height="20"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M5 12l5 5L20 7" />
        </svg>
        <svg
          v-else
          viewBox="0 0 24 24"
          width="20"
          height="20"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <rect
            x="9"
            y="9"
            width="11"
            height="11"
            rx="2"
          />
          <path d="M5 15V6a2 2 0 0 1 2-2h9" />
        </svg>
      </button>
    </Transition>
  </footer>
</template>
