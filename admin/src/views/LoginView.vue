<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import Message from 'primevue/message'
import Password from 'primevue/password'
import ProgressSpinner from 'primevue/progressspinner'

import { fetchBootstrapStatus, formatApiError } from '../api'
import { useAuth } from '../auth'

const route = useRoute()
const router = useRouter()
const { loginWithPassword, registerOwner } = useAuth()
const loading = ref(true)
const sending = ref(false)
const hasUsers = ref(true)
const email = ref('')
const password = ref('')
const tenantName = ref('Default tenant')
const errorMessage = ref('')
const isSetupMode = computed(() => !hasUsers.value)
const canSubmit = computed(() => Boolean(email.value.trim() && password.value.trim()))

async function loadBootstrapStatus() {
  loading.value = true
  errorMessage.value = ''

  try {
    const status = await fetchBootstrapStatus()
    hasUsers.value = status.has_users
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Не удалось проверить состояние gateway')
  } finally {
    loading.value = false
  }
}

async function submit() {
  if (!canSubmit.value) {
    return
  }

  sending.value = true
  errorMessage.value = ''

  try {
    if (isSetupMode.value) {
      await registerOwner(email.value.trim(), password.value, tenantName.value.trim())
    } else {
      await loginWithPassword(email.value.trim(), password.value)
    }

    const redirect = typeof route.query.redirect === 'string' ? route.query.redirect : '/requests'
    router.push(redirect)
  } catch (error) {
    errorMessage.value = formatApiError(error, 'Авторизация не удалась')
  } finally {
    sending.value = false
  }
}

onMounted(loadBootstrapStatus)
</script>

<template>
  <div class="auth-page">
    <div class="auth-panel">
      <div class="auth-logo">
        <i class="pi pi-bolt" />
        <span>AI Gateway</span>
      </div>

      <div v-if="loading" class="empty-state auth-loading">
        <ProgressSpinner stroke-width="4" />
      </div>

      <form v-else class="admin-form" @submit.prevent="submit">
        <div>
          <h2 class="m-0">{{ isSetupMode ? 'Первичная регистрация' : 'Вход' }}</h2>
          <div class="text-muted mt-4">
            {{ isSetupMode ? 'Первый пользователь станет owner' : 'Свободная регистрация закрыта' }}
          </div>
        </div>

        <Message v-if="errorMessage" severity="error" :closable="false">
          {{ errorMessage }}
        </Message>

        <div class="field">
          <label for="email">Email</label>
          <InputText id="email" v-model="email" type="email" autofocus autocomplete="username" />
        </div>

        <div class="field">
          <label for="password">Password</label>
          <Password
            id="password"
            v-model="password"
            :feedback="isSetupMode"
            toggle-mask
            autocomplete="current-password"
          />
        </div>

        <div v-if="isSetupMode" class="field">
          <label for="tenantName">Initial tenant</label>
          <InputText id="tenantName" v-model="tenantName" />
        </div>

        <Button
          type="submit"
          :label="isSetupMode ? 'Создать owner' : 'Войти'"
          icon="pi pi-sign-in"
          class="w-full"
          :loading="sending"
          :disabled="!canSubmit"
        />
      </form>
    </div>
  </div>
</template>
