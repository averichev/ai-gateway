<script setup lang="ts">
import Drawer from 'primevue/drawer'

defineProps<{
  visible: boolean
  header: string
}>()

defineEmits<{
  'update:visible': [value: boolean]
}>()
</script>

<template>
  <Drawer
    :visible="visible"
    :header="header"
    position="right"
    class="admin-help-drawer"
    @update:visible="$emit('update:visible', $event)"
  >
    <div class="help-drawer-content">
      <slot />
    </div>
  </Drawer>
</template>
