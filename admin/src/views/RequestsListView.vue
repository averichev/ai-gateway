<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import Button from 'primevue/button'
import Card from 'primevue/card'
import Column from 'primevue/column'
import DataTable from 'primevue/datatable'
import Message from 'primevue/message'
import ProgressSpinner from 'primevue/progressspinner'
import Tag from 'primevue/tag'

import { fetchRequests, formatDateTime, severityForStatus, type RequestListItem } from '../api'

const router = useRouter()
const loading = ref(true)
const errorMessage = ref('')
const items = ref<RequestListItem[]>([])

async function loadRequests() {
  loading.value = true
  errorMessage.value = ''

  try {
    items.value = await fetchRequests()
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Не удалось загрузить список запросов'
  } finally {
    loading.value = false
  }
}

function openDetails(id: string) {
  router.push(`/requests/${id}`)
}

onMounted(loadRequests)
</script>

<template>
  <section class="page-grid">
    <Card>
      <template #title>Запросы</template>
      <template #subtitle>Последние вызовы gateway и их итоговый маршрут</template>
      <template #content>
        <div class="actions-row">
          <Button label="Обновить" icon="pi pi-refresh" @click="loadRequests" />
        </div>

        <div v-if="loading" class="loading-box">
          <ProgressSpinner stroke-width="4" />
        </div>

        <Message v-else-if="errorMessage" severity="error" :closable="false">
          {{ errorMessage }}
        </Message>

        <DataTable
          v-else
          :value="items"
          paginator
          :rows="15"
          responsive-layout="scroll"
          striped-rows
        >
          <Column field="created_at" header="Время">
            <template #body="{ data }">
              {{ formatDateTime(data.created_at) }}
            </template>
          </Column>

          <Column field="model_alias" header="Alias" />
          <Column field="provider_code" header="Provider" />
          <Column field="external_model" header="Модель" />

          <Column field="status" header="Статус">
            <template #body="{ data }">
              <Tag :value="data.status" :severity="severityForStatus(data.status)" />
            </template>
          </Column>

          <Column field="latency_ms" header="Latency">
            <template #body="{ data }">
              {{ data.latency_ms ?? '-' }}
            </template>
          </Column>

          <Column header="Токены">
            <template #body="{ data }">
              <span>{{ data.input_tokens ?? '-' }} / {{ data.output_tokens ?? '-' }}</span>
            </template>
          </Column>

          <Column header="">
            <template #body="{ data }">
              <Button
                label="Детали"
                size="small"
                text
                icon="pi pi-arrow-right"
                @click="openDetails(data.id)"
              />
            </template>
          </Column>
        </DataTable>
      </template>
    </Card>
  </section>
</template>

<style scoped>
.page-grid {
  display: grid;
}

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
