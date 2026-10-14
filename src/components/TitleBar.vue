<script setup lang="ts">
import { closeWindow, minimizeWindow, toggleMaximizeWindow } from '../api';

defineProps<{ sidebarOpen: boolean }>();
const emit = defineEmits<{ toggleSidebar: [] }>();
</script>

<template>
  <header
    class="titlebar"
    data-tauri-drag-region
  >
    <button
      class="icon-button"
      type="button"
      aria-label="Toggle sidebar"
      :aria-expanded="sidebarOpen"
      @click="emit('toggleSidebar')"
    >
      <svg
        viewBox="0 0 24 24"
        width="18"
        height="18"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
      >
        <path d="M4 6h16M4 12h16M4 18h16" />
      </svg>
    </button>

    <div class="window-controls">
      <button
        class="icon-button"
        type="button"
        aria-label="Minimize"
        @click="minimizeWindow()"
      >
        <svg
          viewBox="0 0 24 24"
          width="16"
          height="16"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
        >
          <path d="M5 12h14" />
        </svg>
      </button>
      <button
        class="icon-button"
        type="button"
        aria-label="Maximize"
        @click="toggleMaximizeWindow()"
      >
        <svg
          viewBox="0 0 24 24"
          width="16"
          height="16"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linejoin="round"
        >
          <rect
            x="5"
            y="5"
            width="14"
            height="14"
            rx="2"
          />
        </svg>
      </button>
      <button
        class="icon-button icon-button--close"
        type="button"
        aria-label="Close"
        @click="closeWindow()"
      >
        <svg
          viewBox="0 0 24 24"
          width="16"
          height="16"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
        >
          <path d="M6 6l12 12M18 6L6 18" />
        </svg>
      </button>
    </div>
  </header>
</template>
