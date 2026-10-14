<script setup lang="ts">
import { computed } from 'vue';
import type { UiState } from '../api';

const props = defineProps<{
  state: UiState;
  loadingModel: boolean;
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
      return 'Stop';
    case 'finalizing':
      return 'Finalizing…';
    case 'done':
      return 'New recording';
  }
});

const inactive = computed(() => props.state === 'noModel' || props.state === 'finalizing');

function recordButtonPress() {
  switch (props.state) {
    case 'noModel':
      if (!props.loadingModel) emit('needModel');
      break;
    case 'idle':
      emit('record');
      break;
    case 'recording':
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
      @click="recordButtonPress"
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
        v-else-if="state === 'recording'"
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
  </footer>
</template>
