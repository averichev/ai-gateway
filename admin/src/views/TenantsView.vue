<script setup lang="ts">
import { FilterMatchMode } from '@primevue/core/api'
import { computed, onMounted, ref } from 'vue'
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
import { createTenant, formatApiError, formatDateTime, type TenantItem } from '../api'
import { formatTenantRole } from '../display'

const { user, tenants, activeTenantId, setActiveTenant, reloadTenants } = useAuth()
const loading = ref(false)
const saving = ref(false)
const errorMessage = ref('')
const dialogVisible = ref(false)
const tenantName = ref('')
const tenantSlug = ref('')
const filters = ref({
  global: { value: null as string | null, matchMode: FilterMatchMode.CONTAINS },
})
const canCreateTenant = computed(() => user.value?.global_role === 'owner')
const canSubmit = computed(() => Boolean(tenantName.value.trim()))

async function loadTenants() {
  loading.value = true
  errorMessage.value = ''

  try {
    await reloadTenants()
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось загрузить организации')
  } finally {
    loading.value = false
  }
}

function openCreateDialog() {
  tenantName.value = ''
  tenantSlug.value = ''
  dialogVisible.value = true
}

async function submitTenant() {
  if (!canSubmit.value) {
    return
  }

  saving.value = true
  errorMessage.value = ''

  try {
    const tenant = await createTenant({
      name: tenantName.value.trim(),
      slug: normalizeOptionalText(tenantSlug.value),
    })
    await reloadTenants()
    setActiveTenant(tenant.id)
    dialogVisible.value = false
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось создать организацию')
  } finally {
    saving.value = false
  }
}

function normalizeOptionalText(value: string): string | undefined {
  const normalized = value.trim()
  return normalized ? normalized : undefined
}

function roleSeverity(role: TenantItem['role']): 'success' | 'info' | 'secondary' {
  return role === 'owner' ? 'success' : role === 'tenant_admin' ? 'info' : 'secondary'
}

onMounted(loadTenants)
</script>

<template>
  <div class="grid grid-cols-12 gap-8">
    <div class="col-span-12">
      <div class="card">
        <Toolbar class="mb-6">
          <template #start>
            <div>
              <h4 class="m-0">Организации</h4>
              <div class="text-muted mt-4">Клиенты, инсталляции и изолированные контуры шлюза</div>
            </div>
          </template>

          <template #end>
            <div class="flex flex-wrap gap-3">
              <Button label="Создать" icon="pi pi-plus" :disabled="!canCreateTenant" @click="openCreateDialog" />
              <Button label="Обновить" icon="pi pi-refresh" severity="secondary" @click="loadTenants" />
            </div>
          </template>
        </Toolbar>

        <Message v-if="errorMessage" severity="error" :closable="false" class="mb-4">
          {{ errorMessage }}
        </Message>

        <DataTable
          v-model:filters="filters"
          :value="tenants"
          :loading="loading"
          data-key="id"
          paginator
          :rows="10"
          :rows-per-page-options="[10, 25, 50]"
          paginator-template="FirstPageLink PrevPageLink PageLinks NextPageLink LastPageLink CurrentPageReport RowsPerPageDropdown"
          current-page-report-template="{first}-{last} из {totalRecords}"
          :global-filter-fields="['name', 'slug', 'role']"
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

          <Column field="name" header="Название" sortable style="min-width: 14rem" />
          <Column field="slug" header="Короткое имя" sortable style="min-width: 12rem" />
          <Column field="role" header="Роль" sortable style="min-width: 10rem">
            <template #body="{ data }">
              <Tag :value="formatTenantRole(data.role)" :severity="roleSeverity(data.role)" />
            </template>
          </Column>
          <Column field="updated_at" header="Обновлено" sortable style="min-width: 12rem">
            <template #body="{ data }">
              {{ formatDateTime(data.updated_at) }}
            </template>
          </Column>
          <Column :exportable="false" style="min-width: 8rem">
            <template #body="{ data }">
              <Button
                :label="data.id === activeTenantId ? 'Активен' : 'Выбрать'"
                :icon="data.id === activeTenantId ? 'pi pi-check' : 'pi pi-arrow-right'"
                severity="secondary"
                :outlined="data.id !== activeTenantId"
                @click="setActiveTenant(data.id)"
              />
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>

  <Dialog v-model:visible="dialogVisible" modal header="Организация" class="admin-dialog">
    <form class="admin-form" @submit.prevent="submitTenant">
      <div class="field">
        <label for="tenantName">Название</label>
        <InputText id="tenantName" v-model="tenantName" autofocus />
      </div>
      <div class="field">
        <label for="tenantSlug">Короткое имя</label>
        <InputText id="tenantSlug" v-model="tenantSlug" placeholder="Необязательно" />
      </div>
      <div class="flex justify-end gap-3">
        <Button type="button" label="Отмена" severity="secondary" text @click="dialogVisible = false" />
        <Button type="submit" label="Создать" icon="pi pi-plus" :loading="saving" :disabled="!canSubmit" />
      </div>
    </form>
  </Dialog>
</template>
