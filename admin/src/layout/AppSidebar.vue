<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { useRoute } from 'vue-router'

import { useLayout } from './composables/layout'
import AppMenu from './AppMenu.vue'

const { layoutState, isDesktop, hasOpenOverlay } = useLayout()
const route = useRoute()
const sidebarRef = ref<HTMLElement | null>(null)
let outsideClickListener: ((event: MouseEvent) => void) | null = null

watch(
  () => route.path,
  (path) => {
    layoutState.activePath = path
    layoutState.overlayMenuActive = false
    layoutState.mobileMenuActive = false
    layoutState.menuHoverActive = false
  },
  { immediate: true },
)

watch(hasOpenOverlay, (open) => {
  if (!isDesktop()) {
    return
  }

  if (open) {
    bindOutsideClickListener()
  } else {
    unbindOutsideClickListener()
  }
})

function bindOutsideClickListener() {
  if (outsideClickListener) {
    return
  }

  outsideClickListener = (event: MouseEvent) => {
    if (isOutsideClicked(event)) {
      layoutState.overlayMenuActive = false
    }
  }

  document.addEventListener('click', outsideClickListener)
}

function unbindOutsideClickListener() {
  if (!outsideClickListener) {
    return
  }

  document.removeEventListener('click', outsideClickListener)
  outsideClickListener = null
}

function isOutsideClicked(event: MouseEvent) {
  const sidebarEl = sidebarRef.value
  const topbarButtonEl = document.querySelector('.layout-menu-button')
  const target = event.target

  if (!sidebarEl || !(target instanceof Node)) {
    return false
  }

  return !(
    sidebarEl.isSameNode(target)
    || sidebarEl.contains(target)
    || topbarButtonEl?.isSameNode(target)
    || topbarButtonEl?.contains(target)
  )
}

onBeforeUnmount(unbindOutsideClickListener)
</script>

<template>
  <aside ref="sidebarRef" class="layout-sidebar">
    <AppMenu />
  </aside>
</template>
