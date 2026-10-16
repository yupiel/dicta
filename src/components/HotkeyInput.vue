<script setup lang="ts">
import { ref } from 'vue';
import type { Hotkey } from '../api';
import { hotkeyFromEvent } from '../hotkey';

defineProps<{ label: string }>();

const emit = defineEmits<{ change: [hotkey: Hotkey] }>();

const capturing = ref(false);

function onKeydown(event: KeyboardEvent) {
  if (!capturing.value) return;
  event.preventDefault();
  if (event.code === 'Escape') {
    capturing.value = false;
    return;
  }
  const hotkey = hotkeyFromEvent(event);
  if (!hotkey) return;
  capturing.value = false;
  emit('change', hotkey);
}
</script>

<template>
  <button
    type="button"
    class="input hotkey-input"
    :data-capturing="capturing"
    @click="capturing = true"
    @keydown="onKeydown"
    @blur="capturing = false"
  >
    {{ capturing ? 'Press the new shortcut… (Esc to cancel)' : label }}
  </button>
</template>
