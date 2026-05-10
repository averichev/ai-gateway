<script setup lang="ts">
import { computed } from 'vue'

import { useLayout } from './composables/layout'
import AppFooter from './AppFooter.vue'
import AppSidebar from './AppSidebar.vue'
import AppTopbar from './AppTopbar.vue'

const { layoutConfig, layoutState, hideMobileMenu } = useLayout()

const containerClass = computed(() => ({
  'layout-overlay': layoutConfig.menuMode === 'overlay',
  'layout-static': layoutConfig.menuMode === 'static',
  'layout-overlay-active': layoutState.overlayMenuActive,
  'layout-mobile-active': layoutState.mobileMenuActive,
  'layout-static-inactive': layoutState.staticMenuInactive,
}))
</script>

<template>
  <div class="layout-wrapper" :class="containerClass">
    <AppTopbar />
    <AppSidebar />

    <div class="layout-main-container">
      <main class="layout-main">
        <RouterView />
      </main>
      <AppFooter />
    </div>

    <div class="layout-mask" @click="hideMobileMenu" />
  </div>
</template>
