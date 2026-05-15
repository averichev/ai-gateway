export type ProviderCatalogStatus = 'supported' | 'planned'

export interface ProviderCatalogModel {
  id: string
  name: string
  note?: string
  recommended?: boolean
  deprecated?: boolean
}

export interface ProviderCatalogItem {
  code: string
  name: string
  kind: string
  baseUrl: string
  status: ProviderCatalogStatus
  statusText: string
  sourceUrl: string
  models: ProviderCatalogModel[]
}

interface ProviderCatalogMatchInput {
  code?: string | null
  kind?: string | null
  base_url?: string | null
  baseUrl?: string | null
}

export const PROVIDER_CATALOG_LAST_REVIEWED = '2026-05-15'

export const PROVIDER_CATALOG: ProviderCatalogItem[] = [
  {
    code: 'openai',
    name: 'OpenAI',
    kind: 'openai-compatible',
    baseUrl: 'https://api.openai.com/v1',
    status: 'supported',
    statusText: 'Поддерживается текущим adapter',
    sourceUrl: 'https://developers.openai.com/api/docs/models',
    models: [
      {
        id: 'gpt-5.5',
        name: 'GPT-5.5',
        note: 'флагманская модель',
      },
      {
        id: 'gpt-5.4',
        name: 'GPT-5.4',
        note: 'сильная универсальная модель',
      },
      {
        id: 'gpt-5.4-mini',
        name: 'GPT-5.4 mini',
        note: 'баланс цены и качества',
        recommended: true,
      },
      {
        id: 'gpt-5.4-nano',
        name: 'GPT-5.4 nano',
        note: 'низкая задержка и цена',
      },
      {
        id: 'gpt-4.1',
        name: 'GPT-4.1',
        note: 'распространённая предыдущая серия',
      },
      {
        id: 'gpt-4.1-mini',
        name: 'GPT-4.1 mini',
        note: 'дешёвый вариант предыдущей серии',
      },
      {
        id: 'gpt-4o',
        name: 'GPT-4o',
        note: 'часто встречается в существующих интеграциях',
      },
      {
        id: 'gpt-4o-mini',
        name: 'GPT-4o mini',
        note: 'часто встречается в существующих интеграциях',
      },
    ],
  },
  {
    code: 'deepseek',
    name: 'DeepSeek',
    kind: 'openai-compatible',
    baseUrl: 'https://api.deepseek.com',
    status: 'supported',
    statusText: 'Поддерживается текущим adapter',
    sourceUrl: 'https://api-docs.deepseek.com/quick_start/pricing/',
    models: [
      {
        id: 'deepseek-v4-flash',
        name: 'DeepSeek-V4-Flash',
        note: 'основная быстрая модель',
        recommended: true,
      },
      {
        id: 'deepseek-v4-pro',
        name: 'DeepSeek-V4-Pro',
        note: 'более сильная модель',
      },
      {
        id: 'deepseek-chat',
        name: 'deepseek-chat',
        note: 'compat alias, будет устаревать',
        deprecated: true,
      },
      {
        id: 'deepseek-reasoner',
        name: 'deepseek-reasoner',
        note: 'compat alias, будет устаревать',
        deprecated: true,
      },
    ],
  },
  {
    code: 'anthropic',
    name: 'Anthropic',
    kind: 'anthropic',
    baseUrl: 'https://api.anthropic.com',
    status: 'planned',
    statusText: 'Нужен отдельный backend adapter',
    sourceUrl: 'https://platform.claude.com/docs/en/about-claude/models/overview',
    models: [
      {
        id: 'claude-opus-4-7',
        name: 'Claude Opus 4.7',
        note: 'самая сильная Claude-модель',
      },
      {
        id: 'claude-sonnet-4-6',
        name: 'Claude Sonnet 4.6',
        note: 'баланс скорости и качества',
        recommended: true,
      },
      {
        id: 'claude-haiku-4-5',
        name: 'Claude Haiku 4.5',
        note: 'быстрая Claude-модель',
      },
      {
        id: 'claude-haiku-4-5-20251001',
        name: 'Claude Haiku 4.5 pinned',
        note: 'пинованный snapshot',
      },
    ],
  },
]

export function supportedProviderCatalogItems(): ProviderCatalogItem[] {
  return PROVIDER_CATALOG.filter((provider) => provider.status === 'supported')
}

export function findProviderCatalogByCode(code: string): ProviderCatalogItem | undefined {
  const normalizedCode = normalize(code)

  return PROVIDER_CATALOG.find((provider) => normalize(provider.code) === normalizedCode)
}

export function matchProviderCatalog(
  provider: ProviderCatalogMatchInput | null | undefined,
): ProviderCatalogItem | undefined {
  if (!provider) {
    return undefined
  }

  const normalizedCode = normalize(provider.code ?? '')
  const normalizedKind = normalize(provider.kind ?? '')
  const normalizedBaseUrl = normalize(provider.base_url ?? provider.baseUrl ?? '')

  return PROVIDER_CATALOG.find((catalogProvider) => {
    if (normalize(catalogProvider.code) === normalizedCode) {
      return true
    }

    if (
      normalizedKind === normalize(catalogProvider.kind) &&
      normalizedBaseUrl.startsWith(normalize(catalogProvider.baseUrl))
    ) {
      return true
    }

    return hostMatches(normalizedBaseUrl, catalogProvider.baseUrl)
  })
}

export function recommendedModelId(provider: ProviderCatalogItem): string {
  return (
    provider.models.find((model) => model.recommended && !model.deprecated)?.id ??
    provider.models.find((model) => !model.deprecated)?.id ??
    provider.models[0]?.id ??
    ''
  )
}

export function formatModelOptionLabel(model: ProviderCatalogModel): string {
  return model.note ? `${model.id} · ${model.note}` : model.id
}

function hostMatches(baseUrl: string, catalogBaseUrl: string): boolean {
  if (!baseUrl) {
    return false
  }

  try {
    const host = new URL(baseUrl).host
    const catalogHost = new URL(catalogBaseUrl).host

    return host === catalogHost || host.endsWith(`.${catalogHost}`)
  } catch {
    return false
  }
}

function normalize(value: string): string {
  return value.trim().toLowerCase().replace(/\/+$/, '')
}
