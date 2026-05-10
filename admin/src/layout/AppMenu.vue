<script setup lang="ts">
import { ref } from 'vue'

import AppMenuItem from './AppMenuItem.vue'
import type { MenuSection } from './menu'

const model = ref<MenuSection[]>([
  {
    label: 'Gateway',
    items: [
      {
        label: 'Запросы',
        icon: 'pi pi-fw pi-list',
        to: '/requests',
      },
      {
        label: 'Проверка',
        icon: 'pi pi-fw pi-send',
        to: '/generate-test',
      },
    ],
  },
  {
    label: 'Конфигурация',
    items: [
      {
        label: 'Providers',
        icon: 'pi pi-fw pi-server',
        to: '/providers',
      },
      {
        label: 'Model Routes',
        icon: 'pi pi-fw pi-sitemap',
        to: '/model-routes',
      },
    ],
  },
])
</script>

<template>
  <ul class="layout-menu">
    <template v-for="(item, index) in model" :key="item.label">
      <AppMenuItem :item="item" :index="index" />
    </template>
  </ul>
</template>
