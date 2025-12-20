/**
 * Serviço de API - Cliente HTTP para comunicação com o backend
 */

import type {
  ApiResponse,
  Backup,
  BackupResult,
  Connection,
  ConnectionTestResult,
  CreateConnectionPayload,
  DashboardStats,
  PaginatedResponse,
  UpdateConnectionPayload,
} from '@/types/api'

const API_BASE = '/api'

/**
 * Classe de erro customizada para erros da API
 */
export class ApiError extends Error {
  constructor(
    message: string,
    public statusCode: number,
    public data?: unknown
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

/**
 * Função auxiliar para fazer requests
 */
async function request<T>(
  endpoint: string,
  options: RequestInit = {}
): Promise<T> {
  const url = `${API_BASE}${endpoint}`

  const defaultHeaders: HeadersInit = {
    'Content-Type': 'application/json',
    Accept: 'application/json',
  }

  const response = await fetch(url, {
    ...options,
    headers: {
      ...defaultHeaders,
      ...options.headers,
    },
  })

  const data = await response.json()

  if (!response.ok) {
    throw new ApiError(
      data.message || data.error || 'Erro na requisição',
      response.status,
      data
    )
  }

  return data
}

/**
 * Serviço de API para conexões
 */
export const connectionsApi = {
  /**
   * Lista todas as conexões
   */
  async list(params?: {
    page?: number
    limit?: number
    type?: string
    status?: string
    search?: string
  }): Promise<PaginatedResponse<Connection>> {
    const searchParams = new URLSearchParams()

    if (params?.page) searchParams.set('page', params.page.toString())
    if (params?.limit) searchParams.set('limit', params.limit.toString())
    if (params?.type) searchParams.set('type', params.type)
    if (params?.status) searchParams.set('status', params.status)
    if (params?.search) searchParams.set('search', params.search)

    const query = searchParams.toString()
    return request<PaginatedResponse<Connection>>(
      `/connections${query ? `?${query}` : ''}`
    )
  },

  /**
   * Obtém uma conexão específica
   */
  async get(id: number): Promise<ApiResponse<Connection>> {
    return request<ApiResponse<Connection>>(`/connections/${id}`)
  },

  /**
   * Cria uma nova conexão
   */
  async create(
    payload: CreateConnectionPayload
  ): Promise<ApiResponse<Connection>> {
    return request<ApiResponse<Connection>>('/connections', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  /**
   * Atualiza uma conexão
   */
  async update(
    id: number,
    payload: UpdateConnectionPayload
  ): Promise<ApiResponse<Connection>> {
    return request<ApiResponse<Connection>>(`/connections/${id}`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    })
  },

  /**
   * Remove uma conexão
   */
  async delete(id: number): Promise<ApiResponse> {
    return request<ApiResponse>(`/connections/${id}`, {
      method: 'DELETE',
    })
  },

  /**
   * Testa a conexão
   */
  async test(id: number): Promise<ApiResponse<ConnectionTestResult>> {
    return request<ApiResponse<ConnectionTestResult>>(
      `/connections/${id}/test`,
      {
        method: 'POST',
      }
    )
  },

  /**
   * Inicia um backup manual
   */
  async backup(id: number): Promise<ApiResponse<BackupResult>> {
    return request<ApiResponse<BackupResult>>(`/connections/${id}/backup`, {
      method: 'POST',
    })
  },
}

/**
 * Serviço de API para backups
 */
export const backupsApi = {
  /**
   * Lista todos os backups
   */
  async list(params?: {
    page?: number
    limit?: number
    status?: string
    connectionId?: number
  }): Promise<PaginatedResponse<Backup>> {
    const searchParams = new URLSearchParams()

    if (params?.page) searchParams.set('page', params.page.toString())
    if (params?.limit) searchParams.set('limit', params.limit.toString())
    if (params?.status) searchParams.set('status', params.status)
    if (params?.connectionId)
      searchParams.set('connectionId', params.connectionId.toString())

    const query = searchParams.toString()
    return request<PaginatedResponse<Backup>>(
      `/backups${query ? `?${query}` : ''}`
    )
  },

  /**
   * Obtém um backup específico
   */
  async get(id: number): Promise<ApiResponse<Backup>> {
    return request<ApiResponse<Backup>>(`/backups/${id}`)
  },

  /**
   * Remove um backup
   */
  async delete(id: number): Promise<ApiResponse> {
    return request<ApiResponse>(`/backups/${id}`, {
      method: 'DELETE',
    })
  },

  /**
   * Retorna a URL de download do backup
   */
  getDownloadUrl(id: number): string {
    return `${API_BASE}/backups/${id}/download`
  },
}

/**
 * Serviço de API para estatísticas
 */
export const statsApi = {
  /**
   * Obtém estatísticas do dashboard
   */
  async get(): Promise<ApiResponse<DashboardStats>> {
    return request<ApiResponse<DashboardStats>>('/stats')
  },
}

/**
 * Health check da API
 */
export async function healthCheck(): Promise<{
  status: string
  timestamp: string
  version: string
}> {
  return request('/health')
}
