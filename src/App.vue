<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { InputDevice, listInputDevices, startRecording, stopRecording } from './api.ts';
import RecordBar from './components/RecordBar.vue';
import SettingsPanel from './components/SettingsPanel.vue';
import TitleBar from './components/TitleBar.vue';
import { useDicta } from './composables/useDicta.ts';

const {
  uiState,
  loadedSource,
  loadingModel,
  models,
  download,
  downloadAndLoad,
  load,
} = useDicta();

const selectedInputDevice = ref<string | undefined>(undefined);
const devices = ref<InputDevice[]>([]);

const sidebarOpen = ref<boolean>(true);
const sidebarView = ref<'history' | 'settings'>('settings');

onMounted(async () => {
  const inputDeviceList = await listInputDevices();
  devices.value = inputDeviceList;
  selectedInputDevice.value = inputDeviceList.find(device => device.isDefault)?.id;
});
</script>

<template>
  <div
    class="app"
    :data-state="uiState"
  >
    <TitleBar
      :sidebar-open="sidebarOpen"
      @toggle-sidebar="sidebarOpen = !sidebarOpen"
    />

    <RecordBar
      :state="uiState"
      :loading-model="loadingModel"
      @record="startRecording({ deviceId: selectedInputDevice })"
      @stop="stopRecording()"
    />

    <SideDrawer
      v-model:open="sidebarOpen"
      v-model:view="sidebarView"
    >
      <!-- TODO: a history view here -->
      <template #settings>
        <SettingsPanel
          v-model:selected-input-device="selectedInputDevice"
          :devices="devices"
          :models="models"
          :download="download"
          :loaded-source="loadedSource"
          :loading="loadingModel"
          @load="load"
          @download="downloadAndLoad"
        />
      </template>
    </SideDrawer>
  </div>
</template>
