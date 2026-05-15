<script setup lang="ts">
import { FilterMatchMode } from '@primevue/core/api'
import { onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import Button from 'primevue/button'
import Column from 'primevue/column'
import DataTable from 'primevue/datatable'
import IconField from 'primevue/iconfield'
import InputIcon from 'primevue/inputicon'
import InputText from 'primevue/inputtext'
import Message from 'primevue/message'
import Tag from 'primevue/tag'
import Toolbar from 'primevue/toolbar'

import { useAuth } from '../auth'
import { fetchRequests, formatApiError, formatDateTime, severityForStatus, type RequestListItem } from '../api'

const router = useRouter()
const { activeTenantId } = useAuth()
const loading = ref(true)
const errorMessage = ref('')
const items = ref<RequestListItem[]>([])
const filters = ref({
  global: { value: null as string | null, matchMode: FilterMatchMode.CONTAINS },
})

async function loadRequests() {
  loading.value = true
  errorMessage.value = ''

  if (!activeTenantId.value) {
    items.value = []
    loading.value = false
    return
  }

  try {
    items.value = await fetchRequests(activeTenantId.value)
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось загрузить список запросов')
  } finally {
    loading.value = false
  }
}

function openDetails(id: string) {
  router.push(`/requests/${id}`)
}

onMounted(loadRequests)
watch(activeTenantId, loadRequests)
</script>

<template>
  <div class="grid grid-cols-12 gap-8">
    <div class="col-span-12">
      <div class="card">
        <Toolbar class="mb-6">
          <template #start>
            <div>
              <h4 class="m-0">Запросы</h4>
              <div class="text-muted mt-4">Последние вызовы выбранного tenant</div>
            </div>
          </template>

          <template #end>
            <Button label="Обновить" icon="pi pi-refresh" severity="secondary" @click="loadRequests" />
          </template>
        </Toolbar>

        <Message v-if="errorMessage" severity="error" :closable="false" class="mb-4">
          {{ errorMessage }}
        </Message>

        <DataTable
          v-model:filters="filters"
          :value="items"
          :loading="loading"
          data-key="id"
          paginator
          :rows="15"
          :rows-per-page-options="[15, 30, 50]"
          paginator-template="FirstPageLink PrevPageLink PageLinks NextPageLink LastPageLink CurrentPageReport RowsPerPageDropdown"
          current-page-report-template="{first}-{last} из {totalRecords}"
          :global-filter-fields="['id', 'model_alias', 'provider_code', 'external_model', 'status', 'error_message']"
          responsive-layout="scroll"
          striped-rows
        >
          <template #header>
            <div class="table-header">
              <h4 class="m-0">Журнал</h4>
              <IconField>
                <InputIcon>
                  <i class="pi pi-search" />
                </InputIcon>
                <InputText v-model="filters.global.value" placeholder="Поиск..." />
              </IconField>
            </div>
          </template>

          <Column field="created_at" header="Время" sortable style="min-width: 12rem">
            <template #body="{ data }">
              {{ formatDateTime(data.created_at) }}
            </template>
          </Column>

          <Column field="model_alias" header="Alias" sortable style="min-width: 10rem" />
          <Column field="gateway_client_name" header="Client" sortable style="min-width: 12rem">
            <template #body="{ data }">
              {{ data.gateway_client_name ?? 'admin' }}
            </template>
          </Column>
          <Column field="provider_code" header="Provider" sortable style="min-width: 10rem" />
          <Column field="external_model" header="Модель" sortable style="min-width: 14rem" />

          <Column field="status" header="Статус" sortable style="min-width: 12rem">
            <template #body="{ data }">
              <Tag :value="data.status" :severity="severityForStatus(data.status)" />
            </template>
          </Column>

          <Column field="latency_ms" header="Latency" sortable style="min-width: 9rem">
            <template #body="{ data }">
              {{ data.latency_ms ?? '-' }}
            </template>
          </Column>

          <Column header="Токены" style="min-width: 9rem">
            <template #body="{ data }">
              {{ data.input_tokens ?? '-' }} / {{ data.output_tokens ?? '-' }}
            </template>
          </Column>

          <Column :exportable="false" style="min-width: 5rem">
            <template #body="{ data }">
              <Button
                icon="pi pi-eye"
                rounded
                outlined
                severity="secondary"
                aria-label="Детали"
                @click="openDetails(data.id)"
              />
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>
</template>
