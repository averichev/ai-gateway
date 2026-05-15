export function formatEnabled(value: boolean): string {
  return value ? 'включён' : 'выключен'
}

export function enabledSeverity(value: boolean): 'success' | 'danger' {
  return value ? 'success' : 'danger'
}

export function formatSecretConfigured(value: boolean): string {
  return value ? 'задан' : 'не задан'
}

export function secretSeverity(value: boolean): 'success' | 'warn' {
  return value ? 'success' : 'warn'
}

export function formatGlobalRole(role: string): string {
  switch (role) {
    case 'owner':
      return 'владелец'
    case 'user':
      return 'пользователь'
    default:
      return role
  }
}

export function formatTenantRole(role: string): string {
  switch (role) {
    case 'owner':
      return 'владелец'
    case 'tenant_admin':
      return 'администратор'
    case 'viewer':
      return 'просмотр'
    default:
      return role
  }
}

export function formatRequestStatus(status: string): string {
  switch (status) {
    case 'success':
      return 'успешно'
    case 'route_not_found':
      return 'маршрут не найден'
    case 'invalid_request':
      return 'ошибка запроса'
    case 'provider_timeout':
      return 'таймаут провайдера'
    case 'provider_error':
      return 'ошибка провайдера'
    case 'provider_invalid_response':
      return 'неверный ответ провайдера'
    case 'provider_transport_error':
      return 'сбой связи с провайдером'
    case 'gateway_misconfigured':
      return 'ошибка настройки шлюза'
    case 'storage_error':
      return 'ошибка хранилища'
    default:
      return status
  }
}

export function formatFinishReason(reason: string): string {
  switch (reason) {
    case 'stop':
      return 'завершено'
    case 'length':
      return 'лимит токенов'
    case 'content_filter':
      return 'фильтр контента'
    case 'tool_calls':
      return 'вызов инструмента'
    default:
      return reason
  }
}
