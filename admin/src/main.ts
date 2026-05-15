import { createApp } from 'vue'
import PrimeVue from 'primevue/config'
import Aura from '@primeuix/themes/aura'

import App from './App.vue'
import router from './router'
import { primevueRuLocale } from './primevueLocale'

import './assets/styles.scss'

const app = createApp(App)

app.use(router)
app.use(PrimeVue, {
  locale: primevueRuLocale,
  theme: {
    preset: Aura,
    options: {
      darkModeSelector: '.app-dark',
    },
  },
})

app.mount('#app')
