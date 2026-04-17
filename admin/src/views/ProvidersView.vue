<script setup lang="ts">
import { onMounted, ref } from 'vue'
import Button from 'primevue/button'
import Card from 'primevue/card'
import Column from 'primevue/column'
import DataTable from 'primevue/datatable'
import Message from 'primevue/message'
import ProgressSpinner from 'primevue/progressspinner'
import Tag from 'primevue/tag'

import { fetchProviders, formatDateTime, type ProviderItem } from '../api'

const loading = ref(true)
const errorMessage = ref('')
const items = ref<ProviderItem[]>([])

async function loadProviders() {
  loading.value = true
  errorMessage.value = ''

  try {
    items.value = await fetchProviders()
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Не удалось загрузить providers'
  } finally {
    loading.value = false
  }
}

onMounted(loadProviders)
</script>

<template>
  <Card>
    <template #title>Providers</template>
    <template #subtitle>
      Текущая конфигурация провайдеров в БД. Редактирование через UI пока сознательно отложено.
    </template>
    <template #content>
      <div class="actions-row">
        <Button label="Обновить" icon="pi pi-refresh" @click="loadProviders" />
      </div>

      <div v-if="loading" class="loading-box">
        <ProgressSpinner stroke-width="4" />
      </div>

      <Message v-else-if="errorMessage" severity="error" :closable="false">
        {{ errorMessage }}
      </Message>

      <DataTable v-else :value="items" responsive-layout="scroll" striped-rows>
        <Column field="code" header="Code" />
        <Column field="kind" header="Kind" />
        <Column field="base_url" header="Base URL" />
        <Column field="api_key_env" header="API key env" />
        <Column field="timeout_ms" header="Timeout ms" />
        <Column field="is_enabled" header="Enabled">
          <template #body="{ data }">
            <Tag :value="data.is_enabled ? 'yes' : 'no'" :severity="data.is_enabled ? 'success' : 'danger'" />
          </template>
        </Column>
        <Column field="updated_at" header="Updated">
          <template #body="{ data }">
            {{ formatDateTime(data.updated_at) }}
          </template>
        </Column>
      </DataTable>
    </template>
  </Card>
</template>

<style scoped>
.actions-row {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 1rem;
}

.loading-box {
  min-height: 220px;
  display: grid;
  place-items: center;
}
</style>
