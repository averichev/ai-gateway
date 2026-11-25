<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import Button from 'primevue/button'
import Dialog from 'primevue/dialog'
import Message from 'primevue/message'
import ProgressBar from 'primevue/progressbar'
import Toolbar from 'primevue/toolbar'

import {
  cancelUpdate, checkUpdate, fetchUpdateSummary, formatApiError, installUpdate, prepareUpdate,
  type UpdateStage, type UpdateSummary,
} from '../api'

const labels: Record<UpdateStage, string> = {
  idle: 'Ожидает проверки', checking: 'Проверка выпуска', available: 'Доступен подписанный выпуск',
  downloading: 'Загрузка', verifying: 'Проверка файла', prepared: 'Готово к установке',
  stopping: 'Завершение запросов', switching: 'Замена файла', starting: 'Запуск шлюза',
  health_check: 'Проверка готовности', completed: 'Обновление установлено',
  rolling_back: 'Восстановление предыдущей версии', rolled_back: 'Предыдущая версия восстановлена',
  failed: 'Операция завершилась ошибкой',
}
const summary = ref<UpdateSummary>()
const status = computed(() => summary.value?.status)
const canCheck = computed(() => status.value && !status.value.recovery_required &&
  ['idle', 'available', 'completed', 'rolled_back', 'failed'].includes(status.value.state))
const phase = computed(() => status.value?.check_outcome === 'no_update' && status.value.state === 'idle'
  ? 'Новых подписанных выпусков нет' : status.value && labels[status.value.state])
const actionError = ref('')
const connectionError = ref('')
const busy = ref(false)
const confirmation = ref<{ id: string; version: string }>()
const acceptedInstall = ref<string>()
const startingInstall = computed(() => !!acceptedInstall.value && status.value?.state === 'prepared' &&
  acceptedInstall.value === status.value.operation_id)
let installAttempt: string | undefined
let timer: ReturnType<typeof setTimeout>
let generation = 0
let mounted = false

async function refresh() {
  clearTimeout(timer)
  const request = ++generation
  try {
    const latest = await fetchUpdateSummary()
    if (request !== generation) return
    summary.value = latest
    connectionError.value = ''
    if (installAttempt && latest.status.operation_id === installAttempt && latest.status.state !== 'prepared') {
      // After a restart the durable operation also resolves a lost HTTP response.
      actionError.value = ''
      acceptedInstall.value = undefined
      installAttempt = undefined
    }
    if (confirmation.value && (latest.status.state !== 'prepared' || latest.status.operation_id !== confirmation.value.id)) {
      confirmation.value = undefined
    }
  } catch (error) {
    if (request === generation) connectionError.value = formatApiError(error, 'Не удалось получить состояние шлюза')
  } finally {
    if (mounted && request === generation && (connectionError.value || summary.value?.installation_mode !== 'disabled')) {
      timer = setTimeout(refresh, connectionError.value || !canCheck.value ? 1500 : 10000)
    }
  }
}

async function perform(action: () => Promise<unknown>) {
  if (busy.value) return
  busy.value = true
  actionError.value = ''
  clearTimeout(timer)
  generation++
  try {
    await action()
  } catch (error) {
    actionError.value = formatApiError(error, 'Не удалось выполнить операцию')
  } finally {
    if (mounted) await refresh()
    busy.value = false
  }
}

function prepare() {
  const current = status.value
  if (current?.operation_id && current.available) {
    const { operation_id, available } = current
    void perform(() => prepareUpdate(operation_id, available.version))
  }
}

function confirmInstall() {
  const id = status.value?.operation_id
  const version = status.value?.available?.version
  if (id && version) confirmation.value = { id, version }
}

function install() {
  const selected = confirmation.value
  confirmation.value = undefined
  if (!selected) return
  installAttempt = selected.id
  void perform(async () => {
    await installUpdate(selected.id)
    acceptedInstall.value = selected.id
  })
}

function cancel() {
  const id = status.value?.operation_id
  if (id) void perform(() => cancelUpdate(id))
}

onMounted(() => { mounted = true; void refresh() })
onUnmounted(() => { mounted = false; generation++; clearTimeout(timer) })
</script>

<template>
  <div class="card">
    <Toolbar class="mb-6">
      <template #start><h4 class="m-0">Обновления шлюза</h4></template>
      <template #end>
        <Button v-if="summary && summary.installation_mode !== 'disabled'" label="Проверить выпуск"
          icon="pi pi-refresh" :disabled="busy || !canCheck || !!connectionError" @click="perform(checkUpdate)" />
      </template>
    </Toolbar>

    <Message v-if="connectionError" severity="warn" class="mb-4" :closable="false">
      Шлюз временно недоступен. Повторяем подключение. {{ connectionError }}
    </Message>
    <Message v-if="actionError" severity="error" class="mb-4" :closable="false">{{ actionError }}</Message>
    <template v-if="summary && status">
      <dl class="summary-grid">
        <div class="summary-item"><dt>Установленная версия</dt><dd>{{ summary.current_version }}</dd></div>
        <div class="summary-item"><dt>Платформа</dt><dd>{{ summary.target }}</dd></div>
        <div v-if="status.available" class="summary-item"><dt>Выбранный выпуск</dt><dd>{{ status.available.version }}</dd></div>
      </dl>
      <Message v-if="summary.installation_reason" severity="info" :closable="false" class="mb-4">
        {{ summary.installation_reason }}
        <span v-if="summary.installation_mode === 'disabled'">Укажите GATEWAY_UPDATE_SOURCE в настройках сервера.</span>
      </Message>
      <template v-if="summary.installation_mode !== 'disabled'">
        <h5 aria-live="polite">{{ startingInstall ? 'Запуск установки' : phase }}</h5>
        <Message v-if="status.state === 'failed' || status.recovery_required" severity="error" :closable="false">
          {{ status.message }} <span v-if="status.recovery_required">Требуется восстановление на сервере.</span>
        </Message>
        <p v-if="status.available?.notes">{{ status.available.notes }}</p>
        <ProgressBar v-if="status.state === 'downloading' && status.total_bytes"
          :value="Math.floor(status.downloaded_bytes * 100 / status.total_bytes)" aria-label="Загружено" />
        <div v-if="summary.installation_supported" class="flex flex-wrap gap-3 mt-4">
          <Button v-if="status.state === 'available'" label="Подготовить" icon="pi pi-download"
            :disabled="busy || !!connectionError" @click="prepare" />
          <template v-if="status.state === 'prepared' && !startingInstall">
            <Button label="Установить" icon="pi pi-check" :disabled="busy || !!connectionError" @click="confirmInstall" />
            <Button label="Отменить подготовку" severity="secondary" :disabled="busy || !!connectionError" @click="cancel" />
          </template>
        </div>
        <Button v-if="status.state === 'completed' || status.state === 'rolled_back'" label="Перезагрузить интерфейс"
          severity="secondary" @click="$router.go(0)" />
      </template>
    </template>
    <ProgressBar v-else-if="!connectionError" mode="indeterminate" aria-label="Загрузка состояния" />
  </div>

  <Dialog :visible="!!confirmation" modal header="Установить обновление?" class="admin-dialog"
    @update:visible="confirmation = undefined">
    Шлюз завершит текущие запросы и перезапустится с версией {{ confirmation?.version }}.
    <template #footer>
      <Button label="Отмена" text @click="confirmation = undefined" />
      <Button label="Установить" :disabled="busy || !!connectionError" @click="install" />
    </template>
  </Dialog>
</template>
