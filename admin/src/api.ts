import axios from 'axios'

export interface RequestListItem {
  id: string
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
  code: string
  kind: string
  base_url: string
  api_key_env: string
  is_enabled: boolean
  timeout_ms: number
  updated_at: string
}

export interface ModelRouteItem {
  alias: string
  provider_code: string
  external_model: string
  is_enabled: boolean
  updated_at: string
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

const api = axios.create({
  baseURL: '/api/admin',
  timeout: 15_000,
})

const generateApi = axios.create({
  baseURL: '/api/v1',
  timeout: 120_000,
})

export async function fetchRequests(limit = 100): Promise<RequestListItem[]> {
  const { data } = await api.get<RequestListItem[]>('/requests', {
    params: { limit },
  })

  return data
}

export async function fetchRequestDetails(id: string): Promise<RequestDetails> {
  const { data } = await api.get<RequestDetails>(`/requests/${id}`)
  return data
}

export async function fetchProviders(): Promise<ProviderItem[]> {
  const { data } = await api.get<ProviderItem[]>('/providers')
  return data
}

export async function fetchModelRoutes(): Promise<ModelRouteItem[]> {
  const { data } = await api.get<ModelRouteItem[]>('/model-routes')
  return data
}

export async function generateText(payload: GenerateRequest): Promise<GenerateResponse> {
  const { data } = await generateApi.post<GenerateResponse>('/generate', payload)
  return data
}

export function formatApiError(error: unknown, fallback: string): string {
  if (axios.isAxiosError<ErrorResponse>(error)) {
    return error.response?.data?.error?.message ?? error.message
  }

  return error instanceof Error ? error.message : fallback
}

export function formatDateTime(value: string): string {
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
