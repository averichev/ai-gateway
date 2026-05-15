<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import Button from 'primevue/button'
import Message from 'primevue/message'
import ProgressSpinner from 'primevue/progressspinner'
import Tag from 'primevue/tag'
import Toolbar from 'primevue/toolbar'

import { useAuth } from '../auth'
import { fetchRequestDetails, formatApiError, formatDateTime, severityForStatus, type RequestDetails } from '../api'

const route = useRoute()
const { activeTenantId } = useAuth()
const loading = ref(true)
const errorMessage = ref('')
const item = ref<RequestDetails | null>(null)

const requestId = computed(() => String(route.params.id ?? ''))

async function loadRequest() {
  loading.value = true
  errorMessage.value = ''

  if (!activeTenantId.value) {
    errorMessage.value = 'Tenant не выбран'
    loading.value = false
    return
  }

  try {
    item.value = await fetchRequestDetails(activeTenantId.value, requestId.value)
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось загрузить детали запроса')
  } finally {
    loading.value = false
  }
}

onMounted(loadRequest)
</script>

<template>
  <div class="grid grid-cols-12 gap-8">
    <div class="col-span-12">
      <div class="card">
        <Toolbar class="mb-6">
          <template #start>
            <RouterLink to="/requests" custom #default="{ navigate }">
              <Button label="К списку" icon="pi pi-arrow-left" severity="secondary" text @click="navigate" />
            </RouterLink>
          </template>

          <template #center>
            <div class="text-muted code-value">{{ requestId }}</div>
          </template>

          <template #end>
            <Button label="Обновить" icon="pi pi-refresh" severity="secondary" @click="loadRequest" />
          </template>
        </Toolbar>

        <div v-if="loading" class="empty-state">
          <ProgressSpinner stroke-width="4" />
        </div>

        <Message v-else-if="errorMessage" severity="error" :closable="false">
          {{ errorMessage }}
        </Message>

        <template v-else-if="item">
          <div class="admin-section-header">
            <div>
              <h4>Детали запроса</h4>
              <div class="text-muted">Маршрут, статус и сохранённые метрики вызова</div>
            </div>
            <Tag :value="item.status" :severity="severityForStatus(item.status)" />
          </div>

          <dl class="summary-grid">
            <div class="summary-item">
              <dt>Время</dt>
              <dd>{{ formatDateTime(item.created_at) }}</dd>
            </div>
            <div class="summary-item">
              <dt>Alias</dt>
              <dd>{{ item.model_alias }}</dd>
            </div>
            <div class="summary-item">
              <dt>Provider</dt>
              <dd>{{ item.provider_code ?? '-' }}</dd>
            </div>
            <div class="summary-item">
              <dt>Client</dt>
              <dd>{{ item.gateway_client_name ?? 'admin' }}</dd>
            </div>
            <div class="summary-item">
              <dt>Модель</dt>
              <dd>{{ item.external_model ?? '-' }}</dd>
            </div>
            <div class="summary-item">
              <dt>Latency</dt>
              <dd>{{ item.latency_ms ?? '-' }} ms</dd>
            </div>
            <div class="summary-item">
              <dt>Input tokens</dt>
              <dd>{{ item.input_tokens ?? '-' }}</dd>
            </div>
            <div class="summary-item">
              <dt>Output tokens</dt>
              <dd>{{ item.output_tokens ?? '-' }}</dd>
            </div>
          </dl>

          <Message v-if="item.error_message" severity="error" :closable="false" class="mt-4">
            {{ item.error_message }}
          </Message>
        </template>
      </div>
    </div>

    <template v-if="item && !loading && !errorMessage">
      <div class="col-span-12 xl:col-span-6">
        <div class="card">
          <div class="font-semibold text-xl mb-4">Prompt preview</div>
          <pre class="preview-surface">{{ item.prompt_preview ?? 'Нет данных' }}</pre>
        </div>
      </div>

      <div class="col-span-12 xl:col-span-6">
        <div class="card">
          <div class="font-semibold text-xl mb-4">Response preview</div>
          <pre class="preview-surface">{{ item.response_preview ?? 'Нет данных' }}</pre>
        </div>
      </div>
    </template>
  </div>
</template>
