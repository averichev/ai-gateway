<script setup lang="ts">
import { FilterMatchMode } from '@primevue/core/api'
import { computed, onMounted, ref, watch } from 'vue'
import Button from 'primevue/button'
import Checkbox from 'primevue/checkbox'
import Column from 'primevue/column'
import DataTable from 'primevue/datatable'
import Dialog from 'primevue/dialog'
import IconField from 'primevue/iconfield'
import InputIcon from 'primevue/inputicon'
import InputText from 'primevue/inputtext'
import Message from 'primevue/message'
import Select from 'primevue/select'
import Tag from 'primevue/tag'
import Toolbar from 'primevue/toolbar'

import { useAuth } from '../auth'
import {
  fetchModelRoutes,
  fetchProviders,
  formatApiError,
  formatDateTime,
  saveModelRoute,
  type ModelRouteItem,
  type ProviderItem,
} from '../api'

const { activeTenantId, canWriteActiveTenant } = useAuth()
const loading = ref(true)
const saving = ref(false)
const errorMessage = ref('')
const items = ref<ModelRouteItem[]>([])
const providers = ref<ProviderItem[]>([])
const filters = ref({
  global: { value: null as string | null, matchMode: FilterMatchMode.CONTAINS },
})
const dialogVisible = ref(false)
const selectedRoute = ref<ModelRouteItem | null>(null)
const form = ref({
  alias: 'smart-default',
  provider_code: '',
  external_model: '',
  is_enabled: true,
})
const providerOptions = computed(() => providers.value.map((provider) => ({
  label: `${provider.code} · ${provider.kind}`,
  value: provider.code,
})))
const canSubmit = computed(() => Boolean(form.value.alias.trim() && form.value.provider_code.trim() && form.value.external_model.trim()))

async function loadModelRoutes() {
  loading.value = true
  errorMessage.value = ''

  if (!activeTenantId.value) {
    items.value = []
    providers.value = []
    loading.value = false
    return
  }

  try {
    const [routesResult, providersResult] = await Promise.all([
      fetchModelRoutes(activeTenantId.value),
      fetchProviders(activeTenantId.value),
    ])
    items.value = routesResult
    providers.value = providersResult
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось загрузить model routes')
  } finally {
    loading.value = false
  }
}

function openCreateDialog() {
  selectedRoute.value = null
  form.value = {
    alias: 'smart-default',
    provider_code: providers.value[0]?.code ?? '',
    external_model: '',
    is_enabled: true,
  }
  dialogVisible.value = true
}

function openEditDialog(route: ModelRouteItem) {
  selectedRoute.value = route
  form.value = {
    alias: route.alias,
    provider_code: route.provider_code,
    external_model: route.external_model,
    is_enabled: route.is_enabled,
  }
  dialogVisible.value = true
}

async function submitRoute() {
  if (!activeTenantId.value || !canSubmit.value) {
    return
  }

  saving.value = true
  errorMessage.value = ''

  try {
    await saveModelRoute(activeTenantId.value, {
      alias: form.value.alias.trim(),
      provider_code: form.value.provider_code.trim(),
      external_model: form.value.external_model.trim(),
      is_enabled: form.value.is_enabled,
    })
    dialogVisible.value = false
    await loadModelRoutes()
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось сохранить model route')
  } finally {
    saving.value = false
  }
}

onMounted(loadModelRoutes)
watch(activeTenantId, loadModelRoutes)
</script>

<template>
  <div class="grid grid-cols-12 gap-8">
    <div class="col-span-12">
      <div class="card">
        <Toolbar class="mb-6">
          <template #start>
            <div>
              <h4 class="m-0">Model Routes</h4>
              <div class="text-muted mt-4">Alias-маршруты выбранного tenant</div>
            </div>
          </template>

          <template #end>
            <div class="flex flex-wrap gap-3">
              <Button label="Создать" icon="pi pi-plus" :disabled="!canWriteActiveTenant" @click="openCreateDialog" />
              <Button label="Обновить" icon="pi pi-refresh" severity="secondary" @click="loadModelRoutes" />
            </div>
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
          <Column :exportable="false" style="min-width: 5rem">
            <template #body="{ data }">
              <Button icon="pi pi-pencil" rounded outlined severity="secondary" aria-label="Изменить" :disabled="!canWriteActiveTenant" @click="openEditDialog(data)" />
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>

  <Dialog v-model:visible="dialogVisible" modal header="Model route" class="admin-dialog">
    <form class="admin-form" @submit.prevent="submitRoute">
      <div class="field">
        <label for="routeAlias">Alias</label>
        <InputText id="routeAlias" v-model="form.alias" />
      </div>
      <div class="field">
        <label for="routeProvider">Provider</label>
        <Select
          id="routeProvider"
          v-model="form.provider_code"
          :options="providerOptions"
          option-label="label"
          option-value="value"
          editable
          filter
          placeholder="Provider code"
        />
      </div>
      <div class="field">
        <label for="routeExternalModel">External model</label>
        <InputText id="routeExternalModel" v-model="form.external_model" />
      </div>
      <div class="field checkbox-field">
        <Checkbox v-model="form.is_enabled" input-id="routeEnabled" binary />
        <label for="routeEnabled">Enabled</label>
      </div>
      <div class="flex justify-end gap-3">
        <Button type="button" label="Отмена" severity="secondary" text @click="dialogVisible = false" />
        <Button type="submit" label="Сохранить" icon="pi pi-save" :loading="saving" :disabled="!canSubmit" />
      </div>
    </form>
  </Dialog>
</template>
