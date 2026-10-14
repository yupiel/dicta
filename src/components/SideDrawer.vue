<script setup lang="ts">
const open = defineModel<boolean>('open', { required: true });
const view = defineModel<'history' | 'settings'>('view', { required: true });
</script>

<template>
  <div
    class="scrim"
    :class="{ 'scrim--visible': open }"
    @click="open = false"
  />
  <aside
    class="drawer"
    :class="{ 'drawer--open': open }"
    :inert="!open"
    @keydown.esc="open = false"
  >
    <div class="drawer__body">
      <slot :name="view" />
    </div>
  </aside>
</template>
