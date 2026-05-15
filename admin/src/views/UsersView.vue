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
import Password from 'primevue/password'
import Select from 'primevue/select'
import Tag from 'primevue/tag'
import Toolbar from 'primevue/toolbar'

import { useAuth } from '../auth'
import {
  createUser,
  fetchUsers,
  formatApiError,
  formatDateTime,
  type AdminUserItem,
} from '../api'

const { user, tenants } = useAuth()
const loading = ref(true)
const saving = ref(false)
const errorMessage = ref('')
const items = ref<AdminUserItem[]>([])
const dialogVisible = ref(false)
const form = ref({
  email: '',
  password: '',
  tenant_id: '',
  tenant_role: 'viewer' as 'tenant_admin' | 'viewer',
})
const filters = ref({
  global: { value: null as string | null, matchMode: FilterMatchMode.CONTAINS },
})
const roleOptions = [
  { label: 'Viewer', value: 'viewer' },
  { label: 'Tenant admin', value: 'tenant_admin' },
]
const tenantOptions = computed(() => tenants.value.map((tenant) => ({ label: tenant.name, value: tenant.id })))
const canCreateUser = computed(() => user.value?.global_role === 'owner')
const canSubmit = computed(() => Boolean(form.value.email.trim() && form.value.password.trim()))

async function loadUsers() {
  loading.value = true
  errorMessage.value = ''

  try {
    items.value = await fetchUsers()
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось загрузить users')
  } finally {
    loading.value = false
  }
}

function openCreateDialog() {
  form.value = {
    email: '',
    password: '',
    tenant_id: tenants.value[0]?.id ?? '',
    tenant_role: 'viewer',
  }
  dialogVisible.value = true
}

async function submitUser() {
  if (!canSubmit.value) {
    return
  }

  saving.value = true
  errorMessage.value = ''

  try {
    await createUser({
      email: form.value.email.trim(),
      password: form.value.password,
      tenant_id: form.value.tenant_id || undefined,
      tenant_role: form.value.tenant_id ? form.value.tenant_role : undefined,
    })
    dialogVisible.value = false
    await loadUsers()
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось создать user')
  } finally {
    saving.value = false
  }
}

onMounted(loadUsers)
</script>

<template>
  <div class="grid grid-cols-12 gap-8">
    <div class="col-span-12">
      <div class="card">
        <Toolbar class="mb-6">
          <template #start>
            <div>
              <h4 class="m-0">Users</h4>
              <div class="text-muted mt-4">Admin users и членство в tenants</div>
            </div>
          </template>

          <template #end>
            <div class="flex flex-wrap gap-3">
              <Button label="Создать" icon="pi pi-plus" :disabled="!canCreateUser" @click="openCreateDialog" />
              <Button label="Обновить" icon="pi pi-refresh" severity="secondary" @click="loadUsers" />
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
          :global-filter-fields="['email', 'global_role']"
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

          <Column field="email" header="Email" sortable style="min-width: 16rem" />
          <Column field="global_role" header="Global role" sortable style="min-width: 10rem">
            <template #body="{ data }">
              <Tag :value="data.global_role" :severity="data.global_role === 'owner' ? 'success' : 'secondary'" />
            </template>
          </Column>
          <Column field="tenant_count" header="Tenants" sortable style="min-width: 9rem" />
          <Column field="is_enabled" header="Enabled" sortable style="min-width: 9rem">
            <template #body="{ data }">
              <Tag :value="data.is_enabled ? 'enabled' : 'disabled'" :severity="data.is_enabled ? 'success' : 'danger'" />
            </template>
          </Column>
          <Column field="created_at" header="Created" sortable style="min-width: 12rem">
            <template #body="{ data }">
              {{ formatDateTime(data.created_at) }}
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>

  <Dialog v-model:visible="dialogVisible" modal header="User" class="admin-dialog">
    <form class="admin-form" @submit.prevent="submitUser">
      <div class="field">
        <label for="userEmail">Email</label>
        <InputText id="userEmail" v-model="form.email" type="email" autofocus />
      </div>
      <div class="field">
        <label for="userPassword">Initial password</label>
        <Password id="userPassword" v-model="form.password" feedback toggle-mask />
      </div>
      <div class="field">
        <label for="userTenant">Tenant</label>
        <Select
          id="userTenant"
          v-model="form.tenant_id"
          :options="tenantOptions"
          option-label="label"
          option-value="value"
          show-clear
          placeholder="Без tenant membership"
        />
      </div>
      <div class="field">
        <label for="userTenantRole">Tenant role</label>
        <Select
          id="userTenantRole"
          v-model="form.tenant_role"
          :options="roleOptions"
          option-label="label"
          option-value="value"
          :disabled="!form.tenant_id"
        />
      </div>
      <div class="flex justify-end gap-3">
        <Button type="button" label="Отмена" severity="secondary" text @click="dialogVisible = false" />
        <Button type="submit" label="Создать" icon="pi pi-plus" :loading="saving" :disabled="!canSubmit" />
      </div>
    </form>
  </Dialog>
</template>
