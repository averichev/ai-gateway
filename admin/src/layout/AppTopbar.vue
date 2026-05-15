<script setup lang="ts">
import Button from 'primevue/button'
import Select from 'primevue/select'
import { useRouter } from 'vue-router'

import { useAuth } from '../auth'
import { useLayout } from './composables/layout'

const { toggleMenu, toggleDarkMode, isDarkTheme } = useLayout()
const { user, tenants, activeTenantId, setActiveTenant, logout } = useAuth()
const router = useRouter()

function signOut() {
  logout()
  router.push('/login')
}
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
      <Select
        v-if="tenants.length"
        :model-value="activeTenantId"
        :options="tenants"
        option-label="name"
        option-value="id"
        class="tenant-select"
        @update:model-value="setActiveTenant"
      />

      <div v-if="user" class="layout-topbar-context">
        <i class="pi pi-user" />
        <span>{{ user.email }}</span>
      </div>

      <button type="button" class="layout-topbar-action" aria-label="Переключить тему" @click="toggleDarkMode">
        <i :class="['pi', isDarkTheme ? 'pi-moon' : 'pi-sun']" />
      </button>

      <Button icon="pi pi-sign-out" text rounded severity="secondary" aria-label="Выйти" @click="signOut" />
    </div>
  </div>
</template>
