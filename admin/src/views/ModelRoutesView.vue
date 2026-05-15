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
import HelpDrawer from '../components/HelpDrawer.vue'
import {
  fetchModelRoutes,
  fetchProviders,
  formatApiError,
  formatDateTime,
  saveModelRoute,
  type ModelRouteItem,
  type ProviderItem,
} from '../api'
import { enabledSeverity, formatEnabled } from '../display'
import {
  PROVIDER_CATALOG_LAST_REVIEWED,
  formatModelOptionLabel,
  matchProviderCatalog,
  recommendedModelId,
  type ProviderCatalogItem,
} from '../providerCatalog'

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
const helpVisible = ref(false)
const selectedRoute = ref<ModelRouteItem | null>(null)
const form = ref({
  alias: 'smart-default',
  provider_code: '',
  external_model: '',
  is_enabled: true,
})
const providerOptions = computed(() =>
  providers.value.map((provider) => {
    const catalogProvider = matchProviderCatalog(provider)

    return {
      label: catalogProvider
        ? `${catalogProvider.name} · ${provider.code}`
        : `${provider.code} · ${provider.kind}`,
      value: provider.code,
    }
  }),
)
const selectedProvider = computed(
  () => providers.value.find((provider) => provider.code === form.value.provider_code) ?? null,
)
const selectedProviderCatalog = computed(() => matchProviderCatalog(selectedProvider.value))
const modelOptions = computed(() => {
  const catalogProvider = selectedProviderCatalog.value

  if (!catalogProvider) {
    return []
  }

  return catalogProvider.models.map((model) => ({
    label: formatModelOptionLabel(model),
    value: model.id,
    name: model.name,
    note: model.note,
    deprecated: Boolean(model.deprecated),
  }))
})
const hasModelCatalog = computed(() => modelOptions.value.length > 0)
const modelCatalogWarning = computed(() => {
  const catalogProvider = selectedProviderCatalog.value

  if (!selectedProvider.value) {
    return ''
  }

  if (!catalogProvider) {
    return 'Для выбранного провайдера нет встроенного списка моделей. Введите внешнее имя вручную.'
  }

  if (catalogProvider.status === 'planned') {
    return `${catalogProvider.name} пока показан справочно: для него нужен отдельный backend adapter.`
  }

  return ''
})
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
    errorMessage.value = formatApiError(error, 'Не удалось загрузить маршруты моделей')
  } finally {
    loading.value = false
  }
}

function openCreateDialog() {
  const providerCode = providers.value[0]?.code ?? ''
  selectedRoute.value = null
  form.value = {
    alias: 'smart-default',
    provider_code: providerCode,
    external_model: defaultModelForProviderCode(providerCode),
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

function defaultModelForProviderCode(providerCode: string): string {
  const catalogProvider = catalogForProviderCode(providerCode)

  if (!catalogProvider || catalogProvider.status !== 'supported') {
    return ''
  }

  return recommendedModelId(catalogProvider)
}

function catalogForProviderCode(providerCode: string): ProviderCatalogItem | undefined {
  const provider = providers.value.find((item) => item.code === providerCode)

  return matchProviderCatalog(provider)
}

function syncModelAfterProviderChange() {
  const nextModel = defaultModelForProviderCode(form.value.provider_code)
  form.value.external_model = nextModel
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
    errorMessage.value = formatApiError(error, 'Не удалось сохранить маршрут модели')
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
              <h4 class="m-0">Маршруты моделей</h4>
              <div class="text-muted mt-4">Связь внутренних алиасов с внешними моделями</div>
            </div>
          </template>

          <template #end>
            <div class="flex flex-wrap gap-3">
              <Button label="Справка" icon="pi pi-question-circle" severity="secondary" outlined @click="helpVisible = true" />
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

          <Column field="alias" header="Алиас" sortable style="min-width: 12rem" />
          <Column field="provider_code" header="Провайдер" sortable style="min-width: 12rem" />
          <Column field="external_model" header="Внешняя модель" sortable style="min-width: 16rem" />
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
          <Column :exportable="false" style="min-width: 5rem">
            <template #body="{ data }">
              <Button icon="pi pi-pencil" rounded outlined severity="secondary" aria-label="Изменить" :disabled="!canWriteActiveTenant" @click="openEditDialog(data)" />
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>

  <Dialog v-model:visible="dialogVisible" modal header="Маршрут модели" class="admin-dialog">
    <form class="admin-form" @submit.prevent="submitRoute">
      <div class="field">
        <label for="routeAlias">Алиас</label>
        <InputText
          id="routeAlias"
          v-model="form.alias"
          aria-describedby="routeAliasHelp"
          placeholder="smart-default"
        />
        <p id="routeAliasHelp" class="field-help">
          Имя, которое используют клиенты в запросе к шлюзу. Его можно оставить стабильным,
          даже если внешняя модель потом поменяется.
        </p>
      </div>
      <div class="field">
        <label for="routeProvider">Провайдер</label>
        <Select
          id="routeProvider"
          v-model="form.provider_code"
          :options="providerOptions"
          option-label="label"
          option-value="value"
          editable
          filter
          aria-describedby="routeProviderHelp"
          placeholder="Код провайдера"
          @change="syncModelAfterProviderChange"
        />
        <p id="routeProviderHelp" class="field-help">
          Подключение из раздела «Провайдеры», через которое шлюз отправит запрос во внешний AI API.
        </p>
      </div>
      <div class="field">
        <label for="routeExternalModel">Внешняя модель</label>
        <Select
          id="routeExternalModel"
          v-model="form.external_model"
          :options="modelOptions"
          option-label="label"
          option-value="value"
          editable
          filter
          aria-describedby="routeExternalModelHelp"
          :placeholder="hasModelCatalog ? 'Выберите модель' : 'Введите модель вручную'"
        >
          <template #option="{ option }">
            <div class="catalog-option">
              <div class="catalog-option-main">
                <span class="code-value">{{ option.value }}</span>
                <Tag v-if="option.deprecated" value="устаревает" severity="warn" />
              </div>
              <small v-if="option.note" class="catalog-option-note">{{ option.note }}</small>
            </div>
          </template>
        </Select>
        <p id="routeExternalModelHelp" class="field-help">
          Точное имя модели у выбранного провайдера. Список зависит от провайдера, но поле
          остаётся редактируемым для новых моделей и совместимых proxy.
        </p>
      </div>
      <Message
        v-if="modelCatalogWarning"
        severity="warn"
        :closable="false"
      >
        {{ modelCatalogWarning }}
      </Message>
      <div class="field checkbox-field">
        <Checkbox v-model="form.is_enabled" input-id="routeEnabled" binary />
        <label for="routeEnabled">Включён</label>
      </div>
      <div class="flex justify-end gap-3">
        <Button type="button" label="Отмена" severity="secondary" text @click="dialogVisible = false" />
        <Button type="submit" label="Сохранить" icon="pi pi-save" :loading="saving" :disabled="!canSubmit" />
      </div>
    </form>
  </Dialog>

  <HelpDrawer v-model:visible="helpVisible" header="Справка: маршруты моделей">
    <section>
      <h5>Что это</h5>
      <p>
        Маршрут модели связывает короткое внутреннее имя с реальной моделью у провайдера.
        Это даёт возможность менять внешнюю модель без изменений в клиентских приложениях.
      </p>
    </section>

    <section>
      <h5>Как используется</h5>
      <p>
        Клиент отправляет в <span class="code-value">POST /api/v1/generate</span> поле
        <span class="code-value">model</span> с алиасом. Шлюз находит маршрут, выбирает
        провайдера и отправляет ему запрос с внешним именем модели.
      </p>
    </section>

    <section>
      <h5>Поля</h5>
      <dl class="help-list">
        <div>
          <dt>Алиас</dt>
          <dd>Имя для ваших клиентов, например <span class="code-value">smart-default</span>. Не привязывайте его к названию конкретной внешней модели.</dd>
        </div>
        <div>
          <dt>Провайдер</dt>
          <dd>Код уже созданного провайдера, например <span class="code-value">openai</span>.</dd>
        </div>
        <div>
          <dt>Внешняя модель</dt>
          <dd>Точное имя модели у провайдера. Для известных провайдеров поле показывает список, сверенный с публичной документацией на <span class="code-value">{{ PROVIDER_CATALOG_LAST_REVIEWED }}</span>.</dd>
        </div>
        <div>
          <dt>Включён</dt>
          <dd>Выключенный маршрут не используется при генерации.</dd>
        </div>
      </dl>
    </section>

    <section>
      <h5>Пример заполнения</h5>
      <pre class="preview-surface help-example">Алиас: smart-default
Провайдер: openai
Внешняя модель: gpt-4o-mini
Включён: да</pre>
    </section>

    <section>
      <h5>Пример запроса клиента</h5>
      <pre class="preview-surface help-example">POST /api/v1/generate
{
  "model": "smart-default",
  "messages": [
    { "role": "user", "content": "Коротко объясни, что делает AI Gateway." }
  ],
  "options": {
    "temperature": 0.2,
    "max_tokens": 200
  }
}</pre>
    </section>
  </HelpDrawer>
</template>
