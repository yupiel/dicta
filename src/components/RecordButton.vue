<script setup lang="ts">
import { ref } from 'vue';

// TODO: figure out possible states
const emit = defineEmits<{
    record: [];
    stop: [];
    reset: [];
    copy: [];
    needModel: [];
}>();

// TODO: more globalized state handling
const isRecording = ref<boolean>(false);

function recordButtonPress() {
    if (!isRecording.value) {
        emit('record');
    } else {
        emit('stop');
    }

    isRecording.value = !isRecording.value;
}
</script>

<template>
  <button
    class='record-button'
    type='button'
    @click='recordButtonPress'
  >
    <svg
      viewBox='0 0 24 24'
      width='20'
      height='20'
      fill='none'
      stroke='currentColor'
      stroke-width='2'
      stroke-linecap='round'
      aria-hidden='true'
    >
      <path d='M12 5v14M5 12h14' />
    </svg>
    {{ isRecording ? 'Stop' : 'Record' }}
  </button>
</template>