<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import Button from 'primevue/button'
import Card from 'primevue/card'
import Message from 'primevue/message'
import ProgressSpinner from 'primevue/progressspinner'
import Tag from 'primevue/tag'

import {
  fetchModelRoutes,
  formatApiError,
  generateText,
  type GenerateMessage,
  type GenerateResponse,
  type ModelRouteItem,
} from '../api'

const routesLoading = ref(true)
const routesError = ref('')
const routes = ref<ModelRouteItem[]>([])
const model = ref('')
const systemPrompt = ref('')
const userPrompt = ref('Скажи коротко: gateway работает?')
const temperature = ref('0.2')
const maxTokens = ref('128')
const sending = ref(false)
const errorMessage = ref('')
const response = ref<GenerateResponse | null>(null)
const latencyMs = ref<number | null>(null)

const enabledRoutes = computed(() => routes.value.filter((route) => route.is_enabled))
const canSubmit = computed(() => {
  return Boolean(
    model.value.trim()
      && userPrompt.value.trim()
      && isOptionalNumber(temperature.value)
      && isOptionalNumber(maxTokens.value)
      && !sending.value,
  )
})

async function loadRoutes() {
  routesLoading.value = true
  routesError.value = ''

  try {
    routes.value = await fetchModelRoutes()
    model.value ||= enabledRoutes.value[0]?.alias ?? ''
  } catch (error) {
    routesError.value = formatApiError(error, 'Не удалось загрузить model routes')
  } finally {
    routesLoading.value = false
  }
}

async function sendRequest() {
  if (!canSubmit.value) {
    return
  }

  sending.value = true
  errorMessage.value = ''
  response.value = null
  latencyMs.value = null

  const messages: GenerateMessage[] = []
  const systemContent = systemPrompt.value.trim()

  if (systemContent) {
    messages.push({ role: 'system', content: systemContent })
  }

  messages.push({ role: 'user', content: userPrompt.value.trim() })

  const options: { temperature?: number; max_tokens?: number } = {}
  const temperatureValue = parseOptionalNumber(temperature.value)
  const maxTokensValue = parseOptionalNumber(maxTokens.value)

  if (temperatureValue !== undefined) {
    options.temperature = temperatureValue
  }

  if (maxTokensValue !== undefined) {
    options.max_tokens = Math.trunc(maxTokensValue)
  }

  const startedAt = performance.now()

  try {
    response.value = await generateText({
      model: model.value.trim(),
      messages,
      options,
    })
    latencyMs.value = Math.round(performance.now() - startedAt)
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Запрос к gateway не удался')
  } finally {
    sending.value = false
  }
}

function clearResponse() {
  response.value = null
  errorMessage.value = ''
  latencyMs.value = null
}

function isOptionalNumber(value: string): boolean {
  return value.trim() === '' || Number.isFinite(Number(value))
}

function parseOptionalNumber(value: string): number | undefined {
  const normalized = value.trim()

  if (!normalized) {
    return undefined
  }

  const parsed = Number(normalized)

  return Number.isFinite(parsed) ? parsed : undefined
}

onMounted(loadRoutes)
</script>

<template>
  <section class="test-grid">
    <Card>
      <template #title>Проверка генерации</template>
      <template #subtitle>Ручной smoke-test полного маршрута через gateway</template>
      <template #content>
        <div class="workspace">
          <form class="request-form" @submit.prevent="sendRequest">
            <Message v-if="routesError" severity="warn" :closable="false">
              {{ routesError }}
            </Message>

            <div class="field-group">
              <label for="model">Model alias</label>
              <div class="model-row">
                <input
                  id="model"
                  v-model="model"
                  class="field-control"
                  list="model-aliases"
                  autocomplete="off"
                />
                <ProgressSpinner v-if="routesLoading" class="route-spinner" stroke-width="4" />
              </div>
              <datalist id="model-aliases">
                <option v-for="route in enabledRoutes" :key="route.alias" :value="route.alias">
                  {{ route.provider_code }} / {{ route.external_model }}
                </option>
              </datalist>
            </div>

            <div class="field-group">
              <label for="systemPrompt">System</label>
              <textarea
                id="systemPrompt"
                v-model="systemPrompt"
                class="field-control textarea-small"
                rows="3"
              />
            </div>

            <div class="field-group">
              <label for="userPrompt">User</label>
              <textarea id="userPrompt" v-model="userPrompt" class="field-control textarea-large" rows="7" />
            </div>

            <div class="options-row">
              <div class="field-group">
                <label for="temperature">Temperature</label>
                <input
                  id="temperature"
                  v-model="temperature"
                  class="field-control"
                  type="number"
                  min="0"
                  max="2"
                  step="0.1"
                />
              </div>

              <div class="field-group">
                <label for="maxTokens">Max tokens</label>
                <input
                  id="maxTokens"
                  v-model="maxTokens"
                  class="field-control"
                  type="number"
                  min="1"
                  step="1"
                />
              </div>
            </div>

            <div class="actions-row">
              <Button type="submit" label="Отправить" icon="pi pi-send" :loading="sending" :disabled="!canSubmit" />
              <Button type="button" label="Очистить" icon="pi pi-times" text @click="clearResponse" />
            </div>
          </form>

          <div class="response-pane">
            <div class="response-header">
              <h3>Ответ</h3>
              <div v-if="response" class="response-meta">
                <Tag :value="response.provider" severity="info" />
                <Tag :value="response.finish_reason" severity="success" />
              </div>
            </div>

            <Message v-if="errorMessage" severity="error" :closable="false">
              {{ errorMessage }}
            </Message>

            <div v-else-if="sending" class="empty-state">
              <ProgressSpinner stroke-width="4" />
            </div>

            <div v-else-if="response" class="response-content">
              <pre>{{ response.output_text }}</pre>
              <dl class="stats-grid">
                <div>
                  <dt>Request ID</dt>
                  <dd>{{ response.id }}</dd>
                </div>
                <div>
                  <dt>Model</dt>
                  <dd>{{ response.model }}</dd>
                </div>
                <div>
                  <dt>Latency</dt>
                  <dd>{{ latencyMs ?? '-' }} ms</dd>
                </div>
                <div>
                  <dt>Tokens</dt>
                  <dd>{{ response.usage?.input_tokens ?? '-' }} / {{ response.usage?.output_tokens ?? '-' }}</dd>
                </div>
              </dl>
            </div>

            <div v-else class="empty-state">
              Нет ответа
            </div>
          </div>
        </div>
      </template>
    </Card>
  </section>
</template>

<style scoped>
.test-grid {
  display: grid;
}

.workspace {
  display: grid;
  grid-template-columns: minmax(320px, 480px) minmax(0, 1fr);
  gap: 1.5rem;
  align-items: start;
}

.request-form {
  display: grid;
  gap: 1rem;
}

.field-group {
  display: grid;
  gap: 0.4rem;
}

.field-group label {
  color: #334155;
  font-size: 0.92rem;
  font-weight: 650;
}

.field-control {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid #cbd5e1;
  border-radius: 8px;
  color: #172033;
  font: inherit;
  padding: 0.72rem 0.8rem;
  background: #ffffff;
}

.field-control:focus {
  border-color: #2563eb;
  box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.16);
  outline: none;
}

.textarea-small,
.textarea-large {
  resize: vertical;
}

.textarea-large {
  min-height: 180px;
}

.model-row {
  position: relative;
}

.route-spinner {
  width: 1.4rem;
  height: 1.4rem;
  position: absolute;
  right: 0.75rem;
  top: 0.75rem;
}

.options-row {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1rem;
}

.actions-row {
  display: flex;
  gap: 0.75rem;
  justify-content: flex-end;
  flex-wrap: wrap;
}

.response-pane {
  min-height: 520px;
  display: grid;
  grid-template-rows: auto 1fr;
  gap: 1rem;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  padding: 1rem;
  background: #f8fafc;
}

.response-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
}

.response-header h3 {
  margin: 0;
  font-size: 1.1rem;
}

.response-meta {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.response-content {
  display: grid;
  gap: 1rem;
}

.response-content pre {
  min-height: 260px;
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  line-height: 1.5;
}

.stats-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 0.75rem;
  margin: 0;
}

.stats-grid div {
  min-width: 0;
}

.stats-grid dt {
  color: #64748b;
  font-size: 0.82rem;
  font-weight: 650;
}

.stats-grid dd {
  margin: 0.2rem 0 0;
  overflow-wrap: anywhere;
}

.empty-state {
  min-height: 320px;
  display: grid;
  place-items: center;
  color: #64748b;
}

@media (max-width: 1100px) {
  .workspace {
    grid-template-columns: 1fr;
  }

  .response-pane {
    min-height: 420px;
  }
}

@media (max-width: 560px) {
  .options-row {
    grid-template-columns: 1fr;
  }

  .actions-row {
    justify-content: stretch;
  }
}
</style>
