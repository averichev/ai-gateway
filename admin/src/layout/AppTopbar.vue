<script setup lang="ts">
import { useLayout } from './composables/layout'

const { toggleMenu, toggleDarkMode, isDarkTheme } = useLayout()
</script>

<template>
  <div class="layout-topbar">
    <div class="layout-topbar-logo-container">
      <button type="button" class="layout-menu-button layout-topbar-action" aria-label="Меню" @click="toggleMenu">
        <i class="pi pi-bars" />
      </button>

      <RouterLink to="/" class="layout-topbar-logo" aria-label="AI Gateway">
        <i class="pi pi-bolt" />
        <span>AI Gateway</span>
      </RouterLink>
    </div>

    <div class="layout-topbar-actions">
      <div class="layout-topbar-context">
        <i class="pi pi-server" />
        <span>Admin</span>
      </div>

      <button type="button" class="layout-topbar-action" aria-label="Переключить тему" @click="toggleDarkMode">
        <i :class="['pi', isDarkTheme ? 'pi-moon' : 'pi-sun']" />
      </button>
    </div>
  </div>
</template>
