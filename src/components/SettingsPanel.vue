<script setup lang="ts">
import type { InputDevice } from '../api';

const selectedInputDevice = defineModel<string | undefined>('selectedInputDevice', { required: true });

defineProps<{
    devices: InputDevice[];
}>();
</script>

<template>
  <section>
    <div class='panel-header'>
      <h2>Settings</h2>
    </div>

    <section>
      <h3>Audio</h3>
      <label
        class='field'
        for='device'
      >
        <span>Microphone</span>
        <select
          id='device'
          v-model='selectedInputDevice'
          class='input'
        >
          <option
            v-for='device in devices'
            :key='device.id'
            :value='device.id'
          >
            {{ device.name }}{{ device.isDefault ? ' (default)' : '' }}
          </option>
        </select>
      </label>
    </section>
  </section>
</template>