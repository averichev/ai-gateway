<script setup lang="ts">
import { FilterMatchMode } from '@primevue/core/api'
import { onMounted, ref } from 'vue'
import Button from 'primevue/button'
import Column from 'primevue/column'
import DataTable from 'primevue/datatable'
import IconField from 'primevue/iconfield'
import InputIcon from 'primevue/inputicon'
import InputText from 'primevue/inputtext'
import Message from 'primevue/message'
import Tag from 'primevue/tag'
import Toolbar from 'primevue/toolbar'

import { fetchModelRoutes, formatDateTime, type ModelRouteItem } from '../api'

const loading = ref(true)
const errorMessage = ref('')
const items = ref<ModelRouteItem[]>([])
const filters = ref({
  global: { value: null as string | null, matchMode: FilterMatchMode.CONTAINS },
})

async function loadModelRoutes() {
  loading.value = true
  errorMessage.value = ''

  try {
    items.value = await fetchModelRoutes()
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Не удалось загрузить model routes'
  } finally {
    loading.value = false
  }
}

onMounted(loadModelRoutes)
</script>

<template>
  <div class="grid grid-cols-12 gap-8">
    <div class="col-span-12">
      <div class="card">
        <Toolbar class="mb-6">
          <template #start>
            <div>
              <h4 class="m-0">Model Routes</h4>
              <div class="text-muted mt-4">Alias-маршруты, через которые клиенты ходят в gateway</div>
            </div>
          </template>

          <template #end>
            <Button label="Обновить" icon="pi pi-refresh" severity="secondary" @click="loadModelRoutes" />
          </template>
        </Toolbar>

        <Message v-if="errorMessage" severity="error" :closable="false" class="mb-4">
          {{ errorMessage }}
        </Message>

        <DataTable
          v-model:filters="filters"
          :value="items"
          :loading="loading"
          data-key="alias"
          paginator
          :rows="10"
          :rows-per-page-options="[10, 25, 50]"
          paginator-template="FirstPageLink PrevPageLink PageLinks NextPageLink LastPageLink CurrentPageReport RowsPerPageDropdown"
          current-page-report-template="{first}-{last} из {totalRecords}"
          :global-filter-fields="['alias', 'provider_code', 'external_model']"
          responsive-layout="scroll"
          striped-rows
        >
          <template #header>
            <div class="table-header">
              <h4 class="m-0">Маршруты</h4>
              <IconField>
                <InputIcon>
                  <i class="pi pi-search" />
                </InputIcon>
                <InputText v-model="filters.global.value" placeholder="Поиск..." />
              </IconField>
            </div>
          </template>

          <Column field="alias" header="Alias" sortable style="min-width: 12rem" />
          <Column field="provider_code" header="Provider" sortable style="min-width: 12rem" />
          <Column field="external_model" header="External model" sortable style="min-width: 16rem" />
          <Column field="is_enabled" header="Enabled" sortable style="min-width: 10rem">
            <template #body="{ data }">
              <Tag :value="data.is_enabled ? 'enabled' : 'disabled'" :severity="data.is_enabled ? 'success' : 'danger'" />
            </template>
          </Column>
          <Column field="updated_at" header="Updated" sortable style="min-width: 12rem">
            <template #body="{ data }">
              {{ formatDateTime(data.updated_at) }}
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>
</template>
