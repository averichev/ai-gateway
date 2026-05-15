<script setup lang="ts">
import { FilterMatchMode } from '@primevue/core/api'
import { computed, onMounted, ref, watch } from 'vue'
import Button from 'primevue/button'
import Column from 'primevue/column'
import DataTable from 'primevue/datatable'
import Dialog from 'primevue/dialog'
import IconField from 'primevue/iconfield'
import InputIcon from 'primevue/inputicon'
import InputText from 'primevue/inputtext'
import Message from 'primevue/message'
import Tag from 'primevue/tag'
import Toolbar from 'primevue/toolbar'

import { useAuth } from '../auth'
import {
  createGatewayClient,
  fetchGatewayClients,
  formatApiError,
  formatDateTime,
  type GatewayClientItem,
} from '../api'
import { enabledSeverity, formatEnabled } from '../display'

const { activeTenantId, canWriteActiveTenant } = useAuth()
const loading = ref(true)
const saving = ref(false)
const errorMessage = ref('')
const items = ref<GatewayClientItem[]>([])
const clientName = ref('')
const createdToken = ref('')
const dialogVisible = ref(false)
const tokenDialogVisible = ref(false)
const filters = ref({
  global: { value: null as string | null, matchMode: FilterMatchMode.CONTAINS },
})
const canSubmit = computed(() => Boolean(clientName.value.trim()))

async function loadClients() {
  loading.value = true
  errorMessage.value = ''

  if (!activeTenantId.value) {
    items.value = []
    loading.value = false
    return
  }

  try {
    items.value = await fetchGatewayClients(activeTenantId.value)
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось загрузить клиентов шлюза')
  } finally {
    loading.value = false
  }
}

function openCreateDialog() {
  clientName.value = ''
  createdToken.value = ''
  dialogVisible.value = true
}

async function submitClient() {
  if (!activeTenantId.value || !canSubmit.value) {
    return
  }

  saving.value = true
  errorMessage.value = ''

  try {
    const created = await createGatewayClient(activeTenantId.value, { name: clientName.value.trim() })
    createdToken.value = created.token
    dialogVisible.value = false
    tokenDialogVisible.value = true
    await loadClients()
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось создать клиента шлюза')
  } finally {
    saving.value = false
  }
}

onMounted(loadClients)
watch(activeTenantId, loadClients)
</script>

<template>
  <div class="grid grid-cols-12 gap-8">
    <div class="col-span-12">
      <div class="card">
        <Toolbar class="mb-6">
          <template #start>
            <div>
              <h4 class="m-0">Клиенты шлюза</h4>
              <div class="text-muted mt-4">Машинные токены для бэкенда выбранной организации</div>
            </div>
          </template>

          <template #end>
            <div class="flex flex-wrap gap-3">
              <Button label="Создать" icon="pi pi-plus" :disabled="!canWriteActiveTenant" @click="openCreateDialog" />
              <Button label="Обновить" icon="pi pi-refresh" severity="secondary" @click="loadClients" />
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
          :global-filter-fields="['name', 'token_prefix']"
          responsive-layout="scroll"
          striped-rows
        >
          <template #header>
            <div class="table-header">
              <h4 class="m-0">Клиенты</h4>
              <IconField>
                <InputIcon>
                  <i class="pi pi-search" />
                </InputIcon>
                <InputText v-model="filters.global.value" placeholder="Поиск..." />
              </IconField>
            </div>
          </template>

          <Column field="name" header="Название" sortable style="min-width: 14rem" />
          <Column field="token_prefix" header="Префикс токена" sortable style="min-width: 12rem">
            <template #body="{ data }">
              <span class="code-value">{{ data.token_prefix }}...</span>
            </template>
          </Column>
          <Column field="is_enabled" header="Статус" sortable style="min-width: 10rem">
            <template #body="{ data }">
              <Tag :value="formatEnabled(data.is_enabled)" :severity="enabledSeverity(data.is_enabled)" />
            </template>
          </Column>
          <Column field="last_used_at" header="Последнее использование" sortable style="min-width: 12rem">
            <template #body="{ data }">
              {{ formatDateTime(data.last_used_at) }}
            </template>
          </Column>
          <Column field="created_at" header="Создан" sortable style="min-width: 12rem">
            <template #body="{ data }">
              {{ formatDateTime(data.created_at) }}
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>

  <Dialog v-model:visible="dialogVisible" modal header="Клиент шлюза" class="admin-dialog">
    <form class="admin-form" @submit.prevent="submitClient">
      <div class="field">
        <label for="clientName">Название</label>
        <InputText id="clientName" v-model="clientName" autofocus />
      </div>
      <div class="flex justify-end gap-3">
        <Button type="button" label="Отмена" severity="secondary" text @click="dialogVisible = false" />
        <Button type="submit" label="Создать" icon="pi pi-plus" :loading="saving" :disabled="!canSubmit" />
      </div>
    </form>
  </Dialog>

  <Dialog v-model:visible="tokenDialogVisible" modal header="Токен клиента" class="admin-dialog">
    <Message severity="warn" :closable="false" class="mb-4">
      Токен показывается один раз. В базе хранится только хэш.
    </Message>
    <pre class="preview-surface token-preview">{{ createdToken }}</pre>
    <div class="flex justify-end mt-4">
      <Button label="Закрыть" @click="tokenDialogVisible = false" />
    </div>
  </Dialog>
</template>
