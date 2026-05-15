<script setup lang="ts">
import { ref } from 'vue'

import AppMenuItem from './AppMenuItem.vue'
import type { MenuSection } from './menu'

const model = ref<MenuSection[]>([
  {
    label: 'Шлюз',
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
        label: 'Провайдеры',
        icon: 'pi pi-fw pi-server',
        to: '/providers',
      },
      {
        label: 'Маршруты моделей',
        icon: 'pi pi-fw pi-sitemap',
        to: '/model-routes',
      },
      {
        label: 'Клиенты шлюза',
        icon: 'pi pi-fw pi-key',
        to: '/gateway-clients',
      },
      {
        label: 'Организации',
        icon: 'pi pi-fw pi-building',
        to: '/tenants',
      },
      {
        label: 'Пользователи',
        icon: 'pi pi-fw pi-users',
        to: '/users',
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
