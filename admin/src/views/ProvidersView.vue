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
import Select from 'primevue/select'
import Tag from 'primevue/tag'
import Toolbar from 'primevue/toolbar'

import { useAuth } from '../auth'
import HelpDrawer from '../components/HelpDrawer.vue'
import {
  fetchProviders,
  formatApiError,
  formatDateTime,
  saveProvider,
  saveProviderSecret,
  type ProviderItem,
} from '../api'
import {
  enabledSeverity,
  formatEnabled,
  formatSecretConfigured,
  secretSeverity,
} from '../display'
import {
  PROVIDER_CATALOG,
  PROVIDER_CATALOG_LAST_REVIEWED,
  findProviderCatalogByCode,
  supportedProviderCatalogItems,
} from '../providerCatalog'

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
const helpVisible = ref(false)
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
const providerPresetOptions = computed(() =>
  PROVIDER_CATALOG.map((provider) => ({
    label:
      provider.status === 'supported'
        ? provider.name
        : `${provider.name} · ${provider.statusText}`,
    value: provider.code,
    disabled: provider.status !== 'supported',
  })),
)
const adapterKindOptions = computed(() => [
  {
    label: 'openai-compatible',
    value: 'openai-compatible',
  },
])
const selectedCatalogProvider = computed(() => findProviderCatalogByCode(form.value.code))
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
    errorMessage.value = formatApiError(error, 'Не удалось загрузить провайдеры')
  } finally {
    loading.value = false
  }
}

function openCreateDialog() {
  const defaultProvider = supportedProviderCatalogItems()[0]
  selectedProvider.value = null
  form.value = {
    code: defaultProvider?.code ?? '',
    kind: defaultProvider?.kind ?? 'openai-compatible',
    base_url: defaultProvider?.baseUrl ?? 'https://api.openai.com/v1',
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

function applyProviderPreset() {
  if (selectedProvider.value) {
    return
  }

  const provider = findProviderCatalogByCode(form.value.code)

  if (!provider || provider.status !== 'supported') {
    return
  }

  form.value.kind = provider.kind
  form.value.base_url = provider.baseUrl
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
    errorMessage.value = formatApiError(error, 'Не удалось сохранить провайдера')
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
    errorMessage.value = formatApiError(error, 'Не удалось сохранить ключ провайдера')
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
              <h4 class="m-0">Провайдеры</h4>
              <div class="text-muted mt-4">Подключения к внешним AI API выбранной организации</div>
            </div>
          </template>

          <template #end>
            <div class="flex flex-wrap gap-3">
              <Button label="Справка" icon="pi pi-question-circle" severity="secondary" outlined @click="helpVisible = true" />
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

          <Column field="code" header="Код" sortable style="min-width: 10rem" />
          <Column field="kind" header="Тип" sortable style="min-width: 12rem" />
          <Column field="base_url" header="Базовый адрес" sortable style="min-width: 18rem" />
          <Column field="api_key_configured" header="API-ключ" sortable style="min-width: 10rem">
            <template #body="{ data }">
              <Tag
                :value="formatSecretConfigured(data.api_key_configured)"
                :severity="secretSeverity(data.api_key_configured)"
              />
            </template>
          </Column>
          <Column field="timeout_ms" header="Таймаут, мс" sortable style="min-width: 10rem" />
          <Column field="is_enabled" header="Статус" sortable style="min-width: 10rem">
            <template #body="{ data }">
              <Tag :value="formatEnabled(data.is_enabled)" :severity="enabledSeverity(data.is_enabled)" />
            </template>
          </Column>
          <Column field="updated_at" header="Обновлено" sortable style="min-width: 12rem">
            <template #body="{ data }">
              {{ formatDateTime(data.updated_at) }}
            </template>
          </Column>
          <Column :exportable="false" style="min-width: 9rem">
            <template #body="{ data }">
              <div class="flex gap-2">
                <Button icon="pi pi-pencil" rounded outlined severity="secondary" aria-label="Изменить" :disabled="!canWriteActiveTenant" @click="openEditDialog(data)" />
                <Button icon="pi pi-lock" rounded outlined severity="secondary" aria-label="API-ключ" :disabled="!canWriteActiveTenant" @click="openSecretDialog(data)" />
              </div>
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>

  <Dialog v-model:visible="dialogVisible" modal header="Провайдер" class="admin-dialog">
    <form class="admin-form" @submit.prevent="submitProvider">
      <div class="field">
        <label for="providerCode">Провайдер</label>
        <Select
          id="providerCode"
          v-model="form.code"
          :options="providerPresetOptions"
          option-label="label"
          option-value="value"
          option-disabled="disabled"
          editable
          filter
          :disabled="Boolean(selectedProvider)"
          aria-describedby="providerCodeHelp"
          placeholder="openai"
          @change="applyProviderPreset"
        />
        <p id="providerCodeHelp" class="field-help">
          Короткое внутреннее имя подключения. Для OpenAI и DeepSeek выберите готовый вариант,
          для совместимого proxy можно ввести свой код вручную.
        </p>
      </div>
      <div class="field">
        <label for="providerKind">Тип</label>
        <Select
          id="providerKind"
          v-model="form.kind"
          :options="adapterKindOptions"
          option-label="label"
          option-value="value"
          editable
          filter
          aria-describedby="providerKindHelp"
          placeholder="openai-compatible"
        />
        <p id="providerKindHelp" class="field-help">
          Адаптер для формата внешнего API. Для OpenAI, DeepSeek и совместимых прокси используйте
          <span class="code-value">openai-compatible</span>.
        </p>
      </div>
      <Message
        v-if="selectedCatalogProvider?.status === 'planned'"
        severity="warn"
        :closable="false"
      >
        {{ selectedCatalogProvider.name }} есть в справочнике моделей, но текущий backend adapter
        ещё не реализован. Такой провайдер не заработает через <span class="code-value">openai-compatible</span>.
      </Message>
      <div class="field">
        <label for="providerBaseUrl">Базовый адрес</label>
        <InputText
          id="providerBaseUrl"
          v-model="form.base_url"
          aria-describedby="providerBaseUrlHelp"
          placeholder="https://api.openai.com/v1"
        />
        <p id="providerBaseUrlHelp" class="field-help">
          Адрес API без <span class="code-value">/chat/completions</span>. Этот путь адаптер добавит сам.
        </p>
      </div>
      <div class="field">
        <label for="providerTimeout">Таймаут, мс</label>
        <InputNumber
          id="providerTimeout"
          v-model="form.timeout_ms"
          :min="1"
          :step="1000"
          show-buttons
          aria-describedby="providerTimeoutHelp"
        />
        <p id="providerTimeoutHelp" class="field-help">
          Сколько шлюз ждёт ответ внешнего API. Обычно хватает 30000-60000 мс.
        </p>
      </div>
      <div class="field checkbox-field">
        <Checkbox v-model="form.is_enabled" input-id="providerEnabled" binary />
        <label for="providerEnabled">Включён</label>
      </div>
      <div class="field">
        <label for="providerApiKey">API-ключ</label>
        <Password
          id="providerApiKey"
          v-model="form.api_key"
          :feedback="false"
          toggle-mask
          aria-describedby="providerApiKeyHelp"
          placeholder="Не менять ключ"
        />
        <p id="providerApiKeyHelp" class="field-help">
          Ключ внешнего сервиса. При редактировании оставьте поле пустым, если ключ не нужно менять.
        </p>
      </div>
      <div class="flex justify-end gap-3">
        <Button type="button" label="Отмена" severity="secondary" text @click="dialogVisible = false" />
        <Button type="submit" label="Сохранить" icon="pi pi-save" :loading="saving" :disabled="!canSubmit" />
      </div>
    </form>
  </Dialog>

  <Dialog v-model:visible="secretDialogVisible" modal header="Ключ провайдера" class="admin-dialog">
    <form class="admin-form" @submit.prevent="submitSecret">
      <div class="field">
        <label for="providerSecret">API-ключ</label>
        <Password
          id="providerSecret"
          v-model="secretValue"
          :feedback="false"
          toggle-mask
          autofocus
          aria-describedby="providerSecretHelp"
        />
        <p id="providerSecretHelp" class="field-help">
          Новый ключ полностью заменит текущий сохранённый ключ провайдера.
        </p>
      </div>
      <div class="flex justify-end gap-3">
        <Button type="button" label="Отмена" severity="secondary" text @click="secretDialogVisible = false" />
        <Button type="submit" label="Сохранить" icon="pi pi-save" :loading="saving" :disabled="!secretValue.trim()" />
      </div>
    </form>
  </Dialog>

  <HelpDrawer v-model:visible="helpVisible" header="Справка: провайдеры">
    <section>
      <h5>Что это</h5>
      <p>
        Провайдер — это подключение шлюза к внешнему AI API. В маршрутах моделей вы выбираете
        провайдера по его коду, а шлюз уже сам отправляет запрос во внешний сервис.
      </p>
    </section>

    <section>
      <h5>Как используется</h5>
      <p>
        Клиент не знает адрес провайдера и не передаёт его ключ. Клиент отправляет запрос в AI Gateway,
        а шлюз берёт нужный провайдер из маршрута модели.
      </p>
    </section>

    <section>
      <h5>Поля</h5>
      <dl class="help-list">
        <div>
          <dt>Провайдер</dt>
          <dd>Короткое внутреннее имя, например <span class="code-value">openai</span>.</dd>
        </div>
        <div>
          <dt>Тип</dt>
          <dd>Адаптер, который понимает API провайдера. Сейчас для OpenAI, DeepSeek и совместимых API используйте <span class="code-value">openai-compatible</span>.</dd>
        </div>
        <div>
          <dt>Базовый адрес</dt>
          <dd>Адрес API без <span class="code-value">/chat/completions</span>. Для OpenAI это <span class="code-value">https://api.openai.com/v1</span>.</dd>
        </div>
        <div>
          <dt>API-ключ</dt>
          <dd>Ключ внешнего сервиса. При редактировании оставьте поле пустым, если ключ менять не нужно.</dd>
        </div>
      </dl>
    </section>

    <section>
      <h5>Встроенный список</h5>
      <p>
        Список провайдеров и моделей сверяется с публичной документацией. Последняя проверка:
        <span class="code-value">{{ PROVIDER_CATALOG_LAST_REVIEWED }}</span>.
      </p>
      <dl class="help-list">
        <div v-for="provider in PROVIDER_CATALOG" :key="provider.code">
          <dt>{{ provider.name }}</dt>
          <dd>
            <span class="code-value">{{ provider.code }}</span>,
            <span class="code-value">{{ provider.kind }}</span>,
            {{ provider.statusText }}.
          </dd>
        </div>
      </dl>
    </section>

    <section>
      <h5>Пример заполнения</h5>
      <pre class="preview-surface help-example">Код: openai
Тип: openai-compatible
Базовый адрес: https://api.openai.com/v1
Таймаут, мс: 60000
Включён: да</pre>
    </section>
  </HelpDrawer>
</template>
