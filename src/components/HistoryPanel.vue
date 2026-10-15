<script setup lang="ts">
import { computed, ref } from 'vue';
import type { HistoryEntry } from '../api';
import ContextMenu from './ContextMenu.vue';

const props = defineProps<{ entries: HistoryEntry[] }>();

const emit = defineEmits<{
  remove: [id: string];
  reveal: [id: string];
  clear: [];
}>();

const SNIPPET_LENGTH = 60;

const newestFirst = ref(true);
const confirmingClear = ref(false);
const menu = ref<{ id: string; x: number; y: number } | null>(null);

const MENU_ITEMS = [
  { action: 'reveal', label: 'Show in folder' },
  { action: 'delete', label: 'Delete', danger: true },
];

function openMenu(event: MouseEvent, id: string) {
  menu.value = { id, x: event.clientX, y: event.clientY };
}

function selectMenuItem(action: string) {
  const target = menu.value;
  menu.value = null;
  if (!target) return;
  if (action === 'reveal') emit('reveal', target.id);
  if (action === 'delete') emit('remove', target.id);
}

const sorted = computed(() => {
  const direction = newestFirst.value ? -1 : 1;
  return [...props.entries].sort((a, b) => direction * (a.createdAtMs - b.createdAtMs));
});

const snippet = (text: string) =>
  text.length > SNIPPET_LENGTH ? `${text.slice(0, SNIPPET_LENGTH).trimEnd()}…` : text;

const formatDate = (createdAtMs: number) =>
  new Date(createdAtMs).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });

const formatTime = (createdAtMs: number) =>
  new Date(createdAtMs).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });

function clear() {
  if (!confirmingClear.value) {
    confirmingClear.value = true;
    return;
  }
  confirmingClear.value = false;
  emit('clear');
}
</script>

<template>
  <section>
    <div class="panel-header">
      <h2>History</h2>
      <button
        class="chip"
        type="button"
        :aria-label="`Sort order: ${newestFirst ? 'newest' : 'oldest'} first`"
        @click="newestFirst = !newestFirst"
      >
        {{ newestFirst ? 'Newest first' : 'Oldest first' }}
        <svg
          viewBox="0 0 24 24"
          width="14"
          height="14"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          :class="{ flipped: !newestFirst }"
        >
          <path d="M12 5v14M6 13l6 6 6-6" />
        </svg>
      </button>
    </div>

    <p
      v-if="sorted.length === 0"
      class="muted"
    >
      Finished transcripts appear here.
    </p>
    <template v-else>
      <ul class="history-list">
        <li
          v-for="entry in sorted"
          :key="entry.id"
        >
          <details
            class="history-item"
            @contextmenu.prevent.stop="openMenu($event, entry.id)"
          >
            <summary>
              <time
                class="history-item__time"
                :datetime="new Date(entry.createdAtMs).toISOString()"
              >
                {{ formatDate(entry.createdAtMs) }} · {{ formatTime(entry.createdAtMs) }}
              </time>
              <span class="history-item__snippet">{{ snippet(entry.text) }}</span>
            </summary>
            <p class="history-item__text">
              {{ entry.text }}
            </p>
            <button
              class="text-button"
              type="button"
              @click="emit('remove', entry.id)"
            >
              Delete
            </button>
          </details>
        </li>
      </ul>
      <button
        class="text-button text-button--danger"
        type="button"
        @click="clear"
        @blur="confirmingClear = false"
      >
        {{ confirmingClear ? 'Really delete all?' : 'Clear history' }}
      </button>
    </template>
    <ContextMenu
      v-if="menu"
      :x="menu.x"
      :y="menu.y"
      :items="MENU_ITEMS"
      @select="selectMenuItem"
      @close="menu = null"
    />
  </section>
</template>
