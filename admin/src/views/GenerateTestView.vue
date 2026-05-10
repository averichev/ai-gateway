<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import Button from 'primevue/button'
import Fluid from 'primevue/fluid'
import InputNumber from 'primevue/inputnumber'
import Message from 'primevue/message'
import Password from 'primevue/password'
import ProgressSpinner from 'primevue/progressspinner'
import Select from 'primevue/select'
import Tag from 'primevue/tag'
import Textarea from 'primevue/textarea'

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
const apiKey = ref('')
const systemPrompt = ref('')
const userPrompt = ref('Скажи коротко: gateway работает?')
const temperature = ref<number | null>(0.2)
const maxTokens = ref<number | null>(128)
const sending = ref(false)
const errorMessage = ref('')
const response = ref<GenerateResponse | null>(null)
const latencyMs = ref<number | null>(null)

const enabledRoutes = computed(() => routes.value.filter((route) => route.is_enabled))
const modelOptions = computed(() => enabledRoutes.value.map((route) => ({
  label: `${route.alias} · ${route.provider_code} / ${route.external_model}`,
  value: route.alias,
})))
const canSubmit = computed(() => Boolean(model.value.trim() && userPrompt.value.trim() && !sending.value))

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

  if (typeof temperature.value === 'number') {
    options.temperature = temperature.value
  }

  if (typeof maxTokens.value === 'number') {
    options.max_tokens = Math.trunc(maxTokens.value)
  }

  const startedAt = performance.now()

  try {
    response.value = await generateText({
      model: model.value.trim(),
      messages,
      api_key: normalizeOptionalText(apiKey.value),
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

function normalizeOptionalText(value: string): string | undefined {
  const normalized = value.trim()

  return normalized ? normalized : undefined
}

onMounted(loadRoutes)
</script>

<template>
  <Fluid>
    <div class="grid grid-cols-12 gap-8">
      <div class="col-span-12 xl:col-span-5">
        <div class="card">
          <div class="admin-section-header">
            <div>
              <h4>Проверка генерации</h4>
              <div class="text-muted">Ручной smoke-test полного маршрута через gateway</div>
            </div>
          </div>

          <Message v-if="routesError" severity="warn" :closable="false" class="mb-4">
            {{ routesError }}
          </Message>

          <form class="admin-form" @submit.prevent="sendRequest">
            <div class="field">
              <label for="model">Model alias</label>
              <Select
                id="model"
                v-model="model"
                :options="modelOptions"
                option-label="label"
                option-value="value"
                editable
                filter
                :loading="routesLoading"
                placeholder="Выберите или введите alias"
              />
            </div>

            <div class="field">
              <label for="apiKey">API key</label>
              <Password
                id="apiKey"
                v-model="apiKey"
                :feedback="false"
                toggle-mask
                placeholder="Env provider по умолчанию"
              />
            </div>

            <div class="field">
              <label for="systemPrompt">System</label>
              <Textarea id="systemPrompt" v-model="systemPrompt" rows="3" auto-resize />
            </div>

            <div class="field">
              <label for="userPrompt">User</label>
              <Textarea id="userPrompt" v-model="userPrompt" rows="8" auto-resize />
            </div>

            <div class="grid grid-cols-12 gap-4">
              <div class="col-span-12 md:col-span-6">
                <div class="field">
                  <label for="temperature">Temperature</label>
                  <InputNumber
                    id="temperature"
                    v-model="temperature"
                    :min="0"
                    :max="2"
                    :step="0.1"
                    :min-fraction-digits="0"
                    :max-fraction-digits="2"
                    show-buttons
                  />
                </div>
              </div>

              <div class="col-span-12 md:col-span-6">
                <div class="field">
                  <label for="maxTokens">Max tokens</label>
                  <InputNumber id="maxTokens" v-model="maxTokens" :min="1" :step="1" show-buttons />
                </div>
              </div>
            </div>

            <div class="flex justify-end flex-wrap gap-3 mt-4">
              <Button type="button" label="Очистить" icon="pi pi-times" severity="secondary" text @click="clearResponse" />
              <Button type="submit" label="Отправить" icon="pi pi-send" :loading="sending" :disabled="!canSubmit" />
            </div>
          </form>
        </div>
      </div>

      <div class="col-span-12 xl:col-span-7">
        <div class="card">
          <div class="admin-section-header">
            <div>
              <h4>Ответ</h4>
              <div class="text-muted">Текст, маршрут и usage по последнему вызову</div>
            </div>

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

          <template v-else-if="response">
            <pre class="preview-surface">{{ response.output_text }}</pre>

            <dl class="summary-grid mt-4">
              <div class="summary-item">
                <dt>Request ID</dt>
                <dd class="code-value">{{ response.id }}</dd>
              </div>
              <div class="summary-item">
                <dt>Model</dt>
                <dd>{{ response.model }}</dd>
              </div>
              <div class="summary-item">
                <dt>Latency</dt>
                <dd>{{ latencyMs ?? '-' }} ms</dd>
              </div>
              <div class="summary-item">
                <dt>Tokens</dt>
                <dd>{{ response.usage?.input_tokens ?? '-' }} / {{ response.usage?.output_tokens ?? '-' }}</dd>
              </div>
            </dl>
          </template>

          <div v-else class="empty-state">
            Нет ответа
          </div>
        </div>
      </div>
    </div>
  </Fluid>
</template>
