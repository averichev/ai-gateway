<script setup lang="ts">
import { useRoute } from 'vue-router'

import AppLayout from './layout/AppLayout.vue'

const route = useRoute()
</script>

<template>
  <RouterView v-if="route.meta.public" />
  <AppLayout v-else />
</template>
