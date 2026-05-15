import { computed, ref } from 'vue'

import {
  bootstrapRegister,
  fetchMe,
  fetchTenants,
  login,
  persistAuthToken,
  readAuthToken,
  type AuthResponse,
  type CurrentUser,
  type TenantItem,
} from './api'

const ACTIVE_TENANT_KEY = 'ai-gateway.admin.tenant'

const initialized = ref(false)
const initializing = ref(false)
const token = ref<string | null>(readAuthToken())
const user = ref<CurrentUser | null>(null)
const tenants = ref<TenantItem[]>([])
const activeTenantId = ref<string | null>(localStorage.getItem(ACTIVE_TENANT_KEY))

const isAuthenticated = computed(() => Boolean(token.value && user.value))
const activeTenant = computed(() => tenants.value.find((tenant) => tenant.id === activeTenantId.value) ?? null)
const canWriteActiveTenant = computed(() => {
  const role = activeTenant.value?.role
  return role === 'owner' || role === 'tenant_admin'
})
let initPromise: Promise<void> | null = null

async function initAuth() {
  if (initialized.value) {
    return
  }

  if (initPromise) {
    await initPromise
    return
  }

  initPromise = runInitAuth()
  await initPromise
}

async function runInitAuth() {
  initializing.value = true

  try {
    if (token.value) {
      const response = await fetchMe()
      applyAuthResponse(response, false)
    }
  } catch {
    clearAuth()
  } finally {
    initialized.value = true
    initializing.value = false
    initPromise = null
  }
}

async function loginWithPassword(email: string, password: string) {
  const response = await login({ email, password })
  applyAuthResponse(response, true)
}

async function registerOwner(email: string, password: string, tenantName?: string) {
  const response = await bootstrapRegister({
    email,
    password,
    tenant_name: tenantName,
  })
  applyAuthResponse(response, true)
}

async function reloadTenants() {
  tenants.value = await fetchTenants()
  ensureActiveTenant()
}

function setActiveTenant(id: string | null) {
  activeTenantId.value = id

  if (id) {
    localStorage.setItem(ACTIVE_TENANT_KEY, id)
  } else {
    localStorage.removeItem(ACTIVE_TENANT_KEY)
  }
}

function logout() {
  clearAuth()
  initialized.value = true
}

function applyAuthResponse(response: AuthResponse, persistToken: boolean) {
  if (persistToken && response.token) {
    token.value = response.token
    persistAuthToken(response.token)
  }

  user.value = response.user
  tenants.value = response.tenants
  ensureActiveTenant()
}

function ensureActiveTenant() {
  if (!tenants.value.length) {
    setActiveTenant(null)
    return
  }

  const exists = tenants.value.some((tenant) => tenant.id === activeTenantId.value)

  if (!exists) {
    setActiveTenant(tenants.value[0].id)
  }
}

function clearAuth() {
  token.value = null
  user.value = null
  tenants.value = []
  setActiveTenant(null)
  persistAuthToken(null)
}

export function useAuth() {
  return {
    initialized,
    initializing,
    token,
    user,
    tenants,
    activeTenantId,
    activeTenant,
    isAuthenticated,
    canWriteActiveTenant,
    initAuth,
    loginWithPassword,
    registerOwner,
    reloadTenants,
    setActiveTenant,
    logout,
  }
}
