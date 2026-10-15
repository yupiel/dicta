<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';

defineProps<{
  x: number;
  y: number;
  items: { action: string; label: string; danger?: boolean }[];
}>();

const emit = defineEmits<{
  select: [action: string];
  close: [];
}>();

const menu = ref<HTMLElement | null>(null);

function closeOnOutsidePress(event: PointerEvent) {
  if (!menu.value?.contains(event.target as Node)) emit('close');
}

function closeOnEscape(event: KeyboardEvent) {
  if (event.key === 'Escape') emit('close');
}

const close = () => emit('close');

onMounted(() => {
  window.addEventListener('pointerdown', closeOnOutsidePress, true);
  window.addEventListener('keydown', closeOnEscape);
  window.addEventListener('blur', close);
  window.addEventListener('resize', close);
  window.addEventListener('scroll', close, true);
  menu.value?.querySelector('button')?.focus();
});

onBeforeUnmount(() => {
  window.removeEventListener('pointerdown', closeOnOutsidePress, true);
  window.removeEventListener('keydown', closeOnEscape);
  window.removeEventListener('blur', close);
  window.removeEventListener('resize', close);
  window.removeEventListener('scroll', close, true);
});
</script>

<template>
  <Teleport to="body">
    <div
      ref="menu"
      class="context-menu"
      role="menu"
      :style="{
        left: `min(${x}px, calc(100vw - 12rem))`,
        top: `min(${y}px, calc(100vh - ${items.length * 2.5 + 1}rem))`,
      }"
    >
      <button
        v-for="item in items"
        :key="item.action"
        class="context-menu__item"
        :class="{ 'context-menu__item--danger': item.danger }"
        type="button"
        role="menuitem"
        @click="emit('select', item.action)"
      >
        {{ item.label }}
      </button>
    </div>
  </Teleport>
</template>
