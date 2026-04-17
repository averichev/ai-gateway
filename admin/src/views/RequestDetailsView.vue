<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import Button from 'primevue/button'
import Card from 'primevue/card'
import Message from 'primevue/message'
import ProgressSpinner from 'primevue/progressspinner'
import Tag from 'primevue/tag'

import { fetchRequestDetails, formatDateTime, severityForStatus, type RequestDetails } from '../api'

const route = useRoute()
const loading = ref(true)
const errorMessage = ref('')
const item = ref<RequestDetails | null>(null)

const requestId = computed(() => String(route.params.id ?? ''))

async function loadRequest() {
  loading.value = true
  errorMessage.value = ''

  try {
    item.value = await fetchRequestDetails(requestId.value)
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Не удалось загрузить детали запроса'
  } finally {
    loading.value = false
  }
}

onMounted(loadRequest)
</script>

<template>
  <section class="details-grid">
    <Card>
      <template #title>Запрос {{ requestId }}</template>
      <template #subtitle>Детали вызова и безопасно сохранённые preview</template>
      <template #content>
        <div class="actions-row">
          <RouterLink to="/requests">
            <Button label="Назад к списку" icon="pi pi-arrow-left" text />
          </RouterLink>
          <Button label="Обновить" icon="pi pi-refresh" @click="loadRequest" />
        </div>

        <div v-if="loading" class="loading-box">
          <ProgressSpinner stroke-width="4" />
        </div>

        <Message v-else-if="errorMessage" severity="error" :closable="false">
          {{ errorMessage }}
        </Message>

        <div v-else-if="item" class="content-grid">
          <div class="summary-grid">
            <div><strong>Время:</strong> {{ formatDateTime(item.created_at) }}</div>
            <div><strong>Alias:</strong> {{ item.model_alias }}</div>
            <div><strong>Provider:</strong> {{ item.provider_code ?? '-' }}</div>
            <div><strong>Модель:</strong> {{ item.external_model ?? '-' }}</div>
            <div>
              <strong>Статус:</strong>
              <Tag :value="item.status" :severity="severityForStatus(item.status)" />
            </div>
            <div><strong>Latency:</strong> {{ item.latency_ms ?? '-' }}</div>
            <div><strong>Input tokens:</strong> {{ item.input_tokens ?? '-' }}</div>
            <div><strong>Output tokens:</strong> {{ item.output_tokens ?? '-' }}</div>
          </div>

          <Message v-if="item.error_message" severity="error" :closable="false">
            {{ item.error_message }}
          </Message>

          <div class="preview-block">
            <h3>Prompt preview</h3>
            <pre>{{ item.prompt_preview ?? 'Нет данных' }}</pre>
          </div>

          <div class="preview-block">
            <h3>Response preview</h3>
            <pre>{{ item.response_preview ?? 'Нет данных' }}</pre>
          </div>
        </div>
      </template>
    </Card>
  </section>
</template>

<style scoped>
.details-grid {
  display: grid;
}

.actions-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  margin-bottom: 1rem;
}

.loading-box {
  min-height: 220px;
  display: grid;
  place-items: center;
}

.content-grid {
  display: grid;
  gap: 1.25rem;
}

.summary-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 0.75rem 1rem;
}

.preview-block {
  background: #f8fafc;
  border-radius: 1rem;
  padding: 1rem;
}

.preview-block h3 {
  margin-top: 0;
}

.preview-block pre {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: ui-monospace, SFMono-Regular, SFMono-Regular, Menlo, monospace;
}
</style>
