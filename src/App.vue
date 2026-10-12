<script setup lang="ts">
import { onMounted, ref } from 'vue';
import SettingsPanel from './components/SettingsPanel.vue';
import { InputDevice, listInputDevices, startRecording } from './api.ts';
import RecordButton from './components/RecordButton.vue';

const selectedInputDevice = ref<string | undefined>(undefined);
const devices = ref<InputDevice[]>([]);

onMounted(async () => {
  const inputDeviceList = await listInputDevices();
  devices.value = inputDeviceList;
  selectedInputDevice.value = inputDeviceList.find(device => device.isDefault)?.id;
});
</script>

<template>
  <div class='app'>
    <SettingsPanel
      v-model:selected-input-device='selectedInputDevice'
      :devices='devices'
    />

    <RecordButton
      @record='startRecording({ deviceId: selectedInputDevice })'
      @stop='console.log(&apos;stopped recording&apos;)'
    />
  </div>
</template>
