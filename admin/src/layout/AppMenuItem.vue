<script setup lang="ts">
import { computed } from 'vue'

import { useLayout } from './composables/layout'
import type { MenuSection } from './menu'

const props = withDefaults(
  defineProps<{
    item: MenuSection
    index?: number
    root?: boolean
    parentPath?: string | null
  }>(),
  {
    index: 0,
    root: true,
    parentPath: null,
  },
)

const { layoutState, isDesktop } = useLayout()

const fullPath = computed(() => {
  if (!props.item.path) {
    return null
  }

  return props.parentPath ? props.parentPath + props.item.path : props.item.path
})

const isActive = computed(() => {
  if (props.item.path && fullPath.value) {
    return layoutState.activePath?.startsWith(fullPath.value)
  }

  return layoutState.activePath === props.item.to
})

function itemClick(event: Event, item: MenuSection) {
  if (item.disabled) {
    event.preventDefault()
    return
  }

  if (item.items) {
    layoutState.activePath = isActive.value ? null : fullPath.value
    layoutState.menuHoverActive = true
    return
  }

  layoutState.overlayMenuActive = false
  layoutState.mobileMenuActive = false
  layoutState.menuHoverActive = false
}

function onMouseEnter() {
  if (isDesktop() && props.root && props.item.items && layoutState.menuHoverActive) {
    layoutState.activePath = fullPath.value
  }
}
</script>

<template>
  <li :class="{ 'layout-root-menuitem': root, 'active-menuitem': isActive }">
    <div v-if="root && item.visible !== false" class="layout-menuitem-root-text">
      {{ item.label }}
    </div>

    <a
      v-if="(!item.to || item.items) && item.visible !== false"
      :href="item.url"
      :class="item.class"
      :target="item.target"
      tabindex="0"
      @click="itemClick($event, item)"
      @mouseenter="onMouseEnter"
    >
      <i :class="item.icon" class="layout-menuitem-icon" />
      <span class="layout-menuitem-text">{{ item.label }}</span>
      <i v-if="item.items" class="pi pi-fw pi-angle-down layout-submenu-toggler" />
    </a>

    <RouterLink
      v-if="item.to && !item.items && item.visible !== false"
      :to="item.to"
      :class="item.class"
      exact-active-class="active-route"
      tabindex="0"
      @click="itemClick($event, item)"
      @mouseenter="onMouseEnter"
    >
      <i :class="item.icon" class="layout-menuitem-icon" />
      <span class="layout-menuitem-text">{{ item.label }}</span>
    </RouterLink>

    <Transition v-if="item.items && item.visible !== false" name="layout-submenu">
      <ul v-show="root || isActive" class="layout-submenu">
        <AppMenuItem
          v-for="child in item.items"
          :key="child.label + '_' + (child.to || child.path)"
          :item="child"
          :root="false"
          :parent-path="fullPath"
        />
      </ul>
    </Transition>
  </li>
</template>
