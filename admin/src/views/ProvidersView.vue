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
import InputNumber from 'primevue/inputnumber'
import InputText from 'primevue/inputtext'
import Message from 'primevue/message'
import Password from 'primevue/password'
import Tag from 'primevue/tag'
import Toolbar from 'primevue/toolbar'

import { useAuth } from '../auth'
import {
  fetchProviders,
  formatApiError,
  formatDateTime,
  saveProvider,
  saveProviderSecret,
  type ProviderItem,
} from '../api'

const { activeTenantId, canWriteActiveTenant } = useAuth()
const loading = ref(true)
const saving = ref(false)
const errorMessage = ref('')
const items = ref<ProviderItem[]>([])
const filters = ref({
  global: { value: null as string | null, matchMode: FilterMatchMode.CONTAINS },
})
const dialogVisible = ref(false)
const secretDialogVisible = ref(false)
const selectedProvider = ref<ProviderItem | null>(null)
const form = ref({
  code: '',
  kind: 'openai-compatible',
  base_url: 'https://api.openai.com/v1',
  is_enabled: true,
  timeout_ms: 60000,
  api_key: '',
})
const secretValue = ref('')
const canSubmit = computed(() => Boolean(form.value.code.trim() && form.value.kind.trim() && form.value.base_url.trim()))

async function loadProviders() {
  loading.value = true
  errorMessage.value = ''

  if (!activeTenantId.value) {
    items.value = []
    loading.value = false
    return
  }

  try {
    items.value = await fetchProviders(activeTenantId.value)
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось загрузить providers')
  } finally {
    loading.value = false
  }
}

function openCreateDialog() {
  selectedProvider.value = null
  form.value = {
    code: '',
    kind: 'openai-compatible',
    base_url: 'https://api.openai.com/v1',
    is_enabled: true,
    timeout_ms: 60000,
    api_key: '',
  }
  dialogVisible.value = true
}

function openEditDialog(provider: ProviderItem) {
  selectedProvider.value = provider
  form.value = {
    code: provider.code,
    kind: provider.kind,
    base_url: provider.base_url,
    is_enabled: provider.is_enabled,
    timeout_ms: provider.timeout_ms,
    api_key: '',
  }
  dialogVisible.value = true
}

function openSecretDialog(provider: ProviderItem) {
  selectedProvider.value = provider
  secretValue.value = ''
  secretDialogVisible.value = true
}

async function submitProvider() {
  if (!activeTenantId.value || !canSubmit.value) {
    return
  }

  saving.value = true
  errorMessage.value = ''

  try {
    await saveProvider(activeTenantId.value, {
      code: form.value.code.trim(),
      kind: form.value.kind.trim(),
      base_url: form.value.base_url.trim(),
      is_enabled: form.value.is_enabled,
      timeout_ms: Math.trunc(form.value.timeout_ms),
      api_key: normalizeOptionalText(form.value.api_key),
    })
    dialogVisible.value = false
    await loadProviders()
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось сохранить provider')
  } finally {
    saving.value = false
  }
}

async function submitSecret() {
  if (!activeTenantId.value || !selectedProvider.value || !secretValue.value.trim()) {
    return
  }

  saving.value = true
  errorMessage.value = ''

  try {
    await saveProviderSecret(activeTenantId.value, selectedProvider.value.id, secretValue.value.trim())
    secretDialogVisible.value = false
    await loadProviders()
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось сохранить provider secret')
  } finally {
    saving.value = false
  }
}

function normalizeOptionalText(value: string): string | undefined {
  const normalized = value.trim()
  return normalized ? normalized : undefined
}

onMounted(loadProviders)
watch(activeTenantId, loadProviders)
</script>

<template>
  <div class="grid grid-cols-12 gap-8">
    <div class="col-span-12">
      <div class="card">
        <Toolbar class="mb-6">
          <template #start>
            <div>
              <h4 class="m-0">Providers</h4>
              <div class="text-muted mt-4">Provider-конфигурация выбранного tenant</div>
            </div>
          </template>

          <template #end>
            <div class="flex flex-wrap gap-3">
              <Button label="Создать" icon="pi pi-plus" :disabled="!canWriteActiveTenant" @click="openCreateDialog" />
              <Button label="Обновить" icon="pi pi-refresh" severity="secondary" @click="loadProviders" />
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
          :global-filter-fields="['code', 'kind', 'base_url']"
          responsive-layout="scroll"
          striped-rows
        >
          <template #header>
            <div class="table-header">
              <h4 class="m-0">Список</h4>
              <IconField>
                <InputIcon>
                  <i class="pi pi-search" />
                </InputIcon>
                <InputText v-model="filters.global.value" placeholder="Поиск..." />
              </IconField>
            </div>
          </template>

          <Column field="code" header="Code" sortable style="min-width: 10rem" />
          <Column field="kind" header="Kind" sortable style="min-width: 12rem" />
          <Column field="base_url" header="Base URL" sortable style="min-width: 18rem" />
          <Column field="api_key_configured" header="Secret" sortable style="min-width: 10rem">
            <template #body="{ data }">
              <Tag
                :value="data.api_key_configured ? 'configured' : 'missing'"
                :severity="data.api_key_configured ? 'success' : 'warn'"
              />
            </template>
          </Column>
          <Column field="timeout_ms" header="Timeout ms" sortable style="min-width: 10rem" />
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
          <Column :exportable="false" style="min-width: 9rem">
            <template #body="{ data }">
              <div class="flex gap-2">
                <Button icon="pi pi-pencil" rounded outlined severity="secondary" aria-label="Изменить" :disabled="!canWriteActiveTenant" @click="openEditDialog(data)" />
                <Button icon="pi pi-lock" rounded outlined severity="secondary" aria-label="Secret" :disabled="!canWriteActiveTenant" @click="openSecretDialog(data)" />
              </div>
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>

  <Dialog v-model:visible="dialogVisible" modal header="Provider" class="admin-dialog">
    <form class="admin-form" @submit.prevent="submitProvider">
      <div class="field">
        <label for="providerCode">Code</label>
        <InputText id="providerCode" v-model="form.code" :disabled="Boolean(selectedProvider)" />
      </div>
      <div class="field">
        <label for="providerKind">Kind</label>
        <InputText id="providerKind" v-model="form.kind" />
      </div>
      <div class="field">
        <label for="providerBaseUrl">Base URL</label>
        <InputText id="providerBaseUrl" v-model="form.base_url" />
      </div>
      <div class="field">
        <label for="providerTimeout">Timeout ms</label>
        <InputNumber id="providerTimeout" v-model="form.timeout_ms" :min="1" :step="1000" show-buttons />
      </div>
      <div class="field checkbox-field">
        <Checkbox v-model="form.is_enabled" input-id="providerEnabled" binary />
        <label for="providerEnabled">Enabled</label>
      </div>
      <div class="field">
        <label for="providerApiKey">API key</label>
        <Password id="providerApiKey" v-model="form.api_key" :feedback="false" toggle-mask placeholder="Не менять secret" />
      </div>
      <div class="flex justify-end gap-3">
        <Button type="button" label="Отмена" severity="secondary" text @click="dialogVisible = false" />
        <Button type="submit" label="Сохранить" icon="pi pi-save" :loading="saving" :disabled="!canSubmit" />
      </div>
    </form>
  </Dialog>

  <Dialog v-model:visible="secretDialogVisible" modal header="Provider secret" class="admin-dialog">
    <form class="admin-form" @submit.prevent="submitSecret">
      <div class="field">
        <label for="providerSecret">API key</label>
        <Password id="providerSecret" v-model="secretValue" :feedback="false" toggle-mask autofocus />
      </div>
      <div class="flex justify-end gap-3">
        <Button type="button" label="Отмена" severity="secondary" text @click="secretDialogVisible = false" />
        <Button type="submit" label="Сохранить" icon="pi pi-save" :loading="saving" :disabled="!secretValue.trim()" />
      </div>
    </form>
  </Dialog>
</template>
