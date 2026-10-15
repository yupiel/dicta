import { createApp } from 'vue';
import App from './App.vue';
import './style.css';

// The only context menu is the custom one on history entries.
document.addEventListener('contextmenu', (event) => event.preventDefault());

createApp(App).mount('#app');
