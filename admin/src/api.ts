import axios from 'axios'

const AUTH_TOKEN_KEY = 'ai-gateway.admin.token'

export interface BootstrapStatus {
  has_users: boolean
}

export interface CurrentUser {
  id: string
  email: string
  global_role: 'owner' | 'user'
}

export interface TenantItem {
  id: string
  name: string
  slug: string
  role: 'owner' | 'tenant_admin' | 'viewer'
  created_at: string
  updated_at: string
}

export interface AdminUserItem {
  id: string
  email: string
  global_role: 'owner' | 'user'
  is_enabled: boolean
  tenant_count: number
  created_at: string
  updated_at: string
}

export interface AuthResponse {
  token: string
  user: CurrentUser
  tenants: TenantItem[]
}

export interface RequestListItem {
  id: string
  tenant_id: string
  gateway_client_id: string | null
  gateway_client_name: string | null
  created_at: string
  model_alias: string
  provider_code: string | null
  external_model: string | null
  status: string
  latency_ms: number | null
  error_message: string | null
  input_tokens: number | null
  output_tokens: number | null
}

export interface RequestDetails extends RequestListItem {
  prompt_preview: string | null
  response_preview: string | null
}

export interface ProviderItem {
  id: string
  tenant_id: string
  code: string
  kind: string
  base_url: string
  api_key_configured: boolean
  is_enabled: boolean
  timeout_ms: number
  updated_at: string
}

export interface ModelRouteItem {
  id: string
  tenant_id: string
  alias: string
  provider_code: string
  external_model: string
  is_enabled: boolean
  updated_at: string
}

export interface GatewayClientItem {
  id: string
  tenant_id: string
  name: string
  token_prefix: string
  is_enabled: boolean
  last_used_at: string | null
  created_at: string
  updated_at: string
}

export interface CreatedGatewayClient extends GatewayClientItem {
  token: string
}

export interface GenerateMessage {
  role: 'system' | 'user' | 'assistant'
  content: string
}

export interface GenerateRequest {
  model: string
  messages: GenerateMessage[]
  options?: {
    temperature?: number
    max_tokens?: number
  }
}

export interface GenerateResponse {
  id: string
  model: string
  provider: string
  output_text: string
  finish_reason: string
  usage?: {
    input_tokens?: number
    output_tokens?: number
  }
}

export interface ErrorResponse {
  request_id: string
  error: {
    code: string
    message: string
  }
}

const authApi = axios.create({
  baseURL: '/api/auth',
  timeout: 15_000,
})

const adminApi = axios.create({
  baseURL: '/api/admin',
  timeout: 15_000,
})

adminApi.interceptors.request.use((config) => {
  const token = localStorage.getItem(AUTH_TOKEN_KEY)

  if (token) {
    config.headers.Authorization = `Bearer ${token}`
  }

  return config
})

authApi.interceptors.request.use((config) => {
  const token = localStorage.getItem(AUTH_TOKEN_KEY)

  if (token) {
    config.headers.Authorization = `Bearer ${token}`
  }

  return config
})

export function persistAuthToken(token: string | null) {
  if (token) {
    localStorage.setItem(AUTH_TOKEN_KEY, token)
  } else {
    localStorage.removeItem(AUTH_TOKEN_KEY)
  }
}

export function readAuthToken(): string | null {
  return localStorage.getItem(AUTH_TOKEN_KEY)
}

export async function fetchBootstrapStatus(): Promise<BootstrapStatus> {
  const { data } = await authApi.get<BootstrapStatus>('/bootstrap')
  return data
}

export async function bootstrapRegister(payload: {
  email: string
  password: string
  tenant_name?: string
}): Promise<AuthResponse> {
  const { data } = await authApi.post<AuthResponse>('/register', payload)
  return data
}

export async function login(payload: { email: string; password: string }): Promise<AuthResponse> {
  const { data } = await authApi.post<AuthResponse>('/login', payload)
  return data
}

export async function fetchMe(): Promise<AuthResponse> {
  const { data } = await authApi.get<AuthResponse>('/me')
  return data
}

export async function fetchTenants(): Promise<TenantItem[]> {
  const { data } = await adminApi.get<TenantItem[]>('/tenants')
  return data
}

export async function createTenant(payload: { name: string; slug?: string }): Promise<TenantItem> {
  const { data } = await adminApi.post<TenantItem>('/tenants', payload)
  return data
}

export async function fetchUsers(): Promise<AdminUserItem[]> {
  const { data } = await adminApi.get<AdminUserItem[]>('/users')
  return data
}

export async function createUser(payload: {
  email: string
  password: string
  tenant_id?: string
  tenant_role?: 'tenant_admin' | 'viewer'
}): Promise<AdminUserItem> {
  const { data } = await adminApi.post<AdminUserItem>('/users', payload)
  return data
}

export async function fetchRequests(tenantId: string, limit = 100): Promise<RequestListItem[]> {
  const { data } = await adminApi.get<RequestListItem[]>(`/tenants/${tenantId}/requests`, {
    params: { limit },
  })

  return data
}

export async function fetchRequestDetails(tenantId: string, id: string): Promise<RequestDetails> {
  const { data } = await adminApi.get<RequestDetails>(`/tenants/${tenantId}/requests/${id}`)
  return data
}

export async function fetchProviders(tenantId: string): Promise<ProviderItem[]> {
  const { data } = await adminApi.get<ProviderItem[]>(`/tenants/${tenantId}/providers`)
  return data
}

export async function saveProvider(
  tenantId: string,
  payload: {
    code: string
    kind: string
    base_url: string
    is_enabled: boolean
    timeout_ms: number
    api_key?: string
  },
): Promise<ProviderItem> {
  const { data } = await adminApi.post<ProviderItem>(`/tenants/${tenantId}/providers`, payload)
  return data
}

export async function saveProviderSecret(
  tenantId: string,
  providerId: string,
  apiKey: string,
): Promise<ProviderItem> {
  const { data } = await adminApi.post<ProviderItem>(
    `/tenants/${tenantId}/providers/${providerId}/secret`,
    { api_key: apiKey },
  )
  return data
}

export async function fetchModelRoutes(tenantId: string): Promise<ModelRouteItem[]> {
  const { data } = await adminApi.get<ModelRouteItem[]>(`/tenants/${tenantId}/model-routes`)
  return data
}

export async function saveModelRoute(
  tenantId: string,
  payload: {
    alias: string
    provider_code: string
    external_model: string
    is_enabled: boolean
  },
): Promise<ModelRouteItem> {
  const { data } = await adminApi.post<ModelRouteItem>(`/tenants/${tenantId}/model-routes`, payload)
  return data
}

export async function fetchGatewayClients(tenantId: string): Promise<GatewayClientItem[]> {
  const { data } = await adminApi.get<GatewayClientItem[]>(`/tenants/${tenantId}/gateway-clients`)
  return data
}

export async function createGatewayClient(
  tenantId: string,
  payload: { name: string },
): Promise<CreatedGatewayClient> {
  const { data } = await adminApi.post<CreatedGatewayClient>(
    `/tenants/${tenantId}/gateway-clients`,
    payload,
  )
  return data
}

export async function generateText(
  tenantId: string,
  payload: GenerateRequest,
): Promise<GenerateResponse> {
  const { data } = await adminApi.post<GenerateResponse>(`/tenants/${tenantId}/generate`, payload, {
    timeout: 120_000,
  })
  return data
}

export function formatApiError(error: unknown, fallback: string): string {
  if (axios.isAxiosError<ErrorResponse>(error)) {
    return error.response?.data?.error?.message ?? error.message
  }

  return error instanceof Error ? error.message : fallback
}

export function formatDateTime(value: string | null): string {
  if (!value) {
    return '-'
  }

  return new Intl.DateTimeFormat('ru-RU', {
    dateStyle: 'short',
    timeStyle: 'medium',
  }).format(new Date(value))
}

export function severityForStatus(status: string): 'success' | 'info' | 'warn' | 'danger' | 'secondary' {
  switch (status) {
    case 'success':
      return 'success'
    case 'route_not_found':
    case 'invalid_request':
      return 'warn'
    case 'provider_timeout':
    case 'provider_error':
    case 'provider_invalid_response':
    case 'provider_transport_error':
    case 'gateway_misconfigured':
    case 'storage_error':
      return 'danger'
    default:
      return 'secondary'
  }
}
