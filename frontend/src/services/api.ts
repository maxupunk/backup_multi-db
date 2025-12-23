/**
 * Serviço de API - Cliente HTTP para comunicação com o backend
 */

import type {
  ApiResponse,
  AuditAction,
  AuditEntityType,
  AuditLog,
  AuditStats,
  AuditStatus,
  Backup,
  BackupResult,
  Connection,
  ConnectionTestResult,
  CreateConnectionPayload,
  CreateStorageDestinationPayload,
  DashboardStats,
  LoginPayload,
  PaginatedResponse,
  RegisterPayload,
  StorageDestination,
  StorageSpaceInfo,
  UpdateConnectionPayload,
  UpdateStorageDestinationPayload,
} from '@/types/api'
import type { AuthResponse, User } from '@/types/auth'

const API_BASE = '/api'

/**
 * Classe de erro customizada para erros da API
 */
export class ApiError extends Error {
  constructor (
    message: string,
    public statusCode: number,
    public data?: unknown,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

/**
 * Extrai mensagem de erro de diferentes formatos de resposta do backend
 */
function extractErrorMessage(data: unknown): string {
  if (typeof data !== 'object' || data === null) {
    return 'Erro na requisição'
  }
  
  const obj = data as Record<string, unknown>
  
  // Formato: { errors: [{ message: "..." }] }
  if (Array.isArray(obj.errors) && obj.errors.length > 0) {
    const firstError = obj.errors[0]
    if (typeof firstError === 'object' && firstError !== null && 'message' in firstError) {
      return String(firstError.message)
    }
  }
  
  // Formato: { message: "..." }
  if (typeof obj.message === 'string') {
    return obj.message
  }
  
  // Formato: { error: "..." }
  if (typeof obj.error === 'string') {
    return obj.error
  }
  
  return 'Erro na requisição'
}

/**
 * Função auxiliar para fazer requests
 */
async function request<T> (
  endpoint: string,
  options: RequestInit = {},
): Promise<T> {
  const url = `${API_BASE}${endpoint}`

  const token = localStorage.getItem('token')
  const authHeaderValue = token ? `Bearer ${token}` : null

  const defaultHeaders: Record<string, string> = {
    'Content-Type': 'application/json',
    'Accept': 'application/json',
  }
  if (authHeaderValue) {
    defaultHeaders.Authorization = authHeaderValue
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
      extractErrorMessage(data),
      response.status,
      data,
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
  async list (params?: {
    page?: number
    limit?: number
    type?: string
    status?: string
    search?: string
  }): Promise<PaginatedResponse<Connection>> {
    const searchParams = new URLSearchParams()

    if (params?.page) {
      searchParams.set('page', params.page.toString())
    }
    if (params?.limit) {
      searchParams.set('limit', params.limit.toString())
    }
    if (params?.type) {
      searchParams.set('type', params.type)
    }
    if (params?.status) {
      searchParams.set('status', params.status)
    }
    if (params?.search) {
      searchParams.set('search', params.search)
    }

    const query = searchParams.toString()
    return request<PaginatedResponse<Connection>>(
      `/connections${query ? `?${query}` : ''}`,
    )
  },

  /**
   * Obtém uma conexão específica
   */
  async get (id: number): Promise<ApiResponse<Connection>> {
    return request<ApiResponse<Connection>>(`/connections/${id}`)
  },

  /**
   * Cria uma nova conexão
   */
  async create (
    payload: CreateConnectionPayload,
  ): Promise<ApiResponse<Connection>> {
    return request<ApiResponse<Connection>>('/connections', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  /**
   * Atualiza uma conexão
   */
  async update (
    id: number,
    payload: UpdateConnectionPayload,
  ): Promise<ApiResponse<Connection>> {
    return request<ApiResponse<Connection>>(`/connections/${id}`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    })
  },

  /**
   * Remove uma conexão
   */
  async delete (id: number): Promise<ApiResponse> {
    return request<ApiResponse>(`/connections/${id}`, {
      method: 'DELETE',
    })
  },

  /**
   * Testa a conexão
   */
  async test (id: number): Promise<ApiResponse<ConnectionTestResult>> {
    return request<ApiResponse<ConnectionTestResult>>(
      `/connections/${id}/test`,
      {
        method: 'POST',
      },
    )
  },

  /**
   * Inicia um backup manual
   */
  async backup (id: number): Promise<ApiResponse<BackupResult>> {
    return request<ApiResponse<BackupResult>>(`/connections/${id}/backup`, {
      method: 'POST',
    })
  },
}

export const storageDestinationsApi = {
  async list (params?: {
    page?: number
    limit?: number
    type?: string
    status?: string
    search?: string
  }): Promise<PaginatedResponse<StorageDestination>> {
    const searchParams = new URLSearchParams()

    if (params?.page) {
      searchParams.set('page', params.page.toString())
    }
    if (params?.limit) {
      searchParams.set('limit', params.limit.toString())
    }
    if (params?.type) {
      searchParams.set('type', params.type)
    }
    if (params?.status) {
      searchParams.set('status', params.status)
    }
    if (params?.search) {
      searchParams.set('search', params.search)
    }

    const query = searchParams.toString()
    return request<PaginatedResponse<StorageDestination>>(
      `/storage-destinations${query ? `?${query}` : ''}`,
    )
  },

  async get (id: number): Promise<ApiResponse<StorageDestination>> {
    return request<ApiResponse<StorageDestination>>(`/storage-destinations/${id}`)
  },

  async create (
    payload: CreateStorageDestinationPayload,
  ): Promise<ApiResponse<StorageDestination>> {
    return request<ApiResponse<StorageDestination>>('/storage-destinations', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  async update (
    id: number,
    payload: UpdateStorageDestinationPayload,
  ): Promise<ApiResponse<StorageDestination>> {
    return request<ApiResponse<StorageDestination>>(`/storage-destinations/${id}`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    })
  },

  async delete (id: number): Promise<ApiResponse> {
    return request<ApiResponse>(`/storage-destinations/${id}`, {
      method: 'DELETE',
    })
  },

  /**
   * Obtém informações de espaço de todos os destinos
   */
  async spaceAll (): Promise<ApiResponse<StorageSpaceInfo[]>> {
    return request<ApiResponse<StorageSpaceInfo[]>>('/storage-destinations-space')
  },

  /**
   * Obtém informações de espaço de um destino específico
   */
  async space (id: number): Promise<ApiResponse<StorageSpaceInfo | null>> {
    return request<ApiResponse<StorageSpaceInfo | null>>(`/storage-destinations/${id}/space`)
  },
}

/**
 * Serviço de API para backups
 */
export const backupsApi = {
  /**
   * Lista todos os backups
   */
  async list (params?: {
    page?: number
    limit?: number
    status?: string
    connectionId?: number
  }): Promise<PaginatedResponse<Backup>> {
    const searchParams = new URLSearchParams()

    if (params?.page) {
      searchParams.set('page', params.page.toString())
    }
    if (params?.limit) {
      searchParams.set('limit', params.limit.toString())
    }
    if (params?.status) {
      searchParams.set('status', params.status)
    }
    if (params?.connectionId) {
      searchParams.set('connectionId', params.connectionId.toString())
    }

    const query = searchParams.toString()
    return request<PaginatedResponse<Backup>>(
      `/backups${query ? `?${query}` : ''}`,
    )
  },

  /**
   * Obtém um backup específico
   */
  async get (id: number): Promise<ApiResponse<Backup>> {
    return request<ApiResponse<Backup>>(`/backups/${id}`)
  },

  /**
   * Remove um backup
   */
  async delete (id: number): Promise<ApiResponse> {
    return request<ApiResponse>(`/backups/${id}`, {
      method: 'DELETE',
    })
  },

  /**
   * Faz download do backup com autenticação
   * @param id ID do backup
   * @param fileName Nome do arquivo para salvar (opcional)
   */
  async download (id: number, fileName?: string): Promise<void> {
    const url = `${API_BASE}/backups/${id}/download`
    const token = localStorage.getItem('token')
    
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
    })
    
    if (!response.ok) {
      const data = await response.json().catch(() => ({}))
      throw new ApiError(
        extractErrorMessage(data),
        response.status,
        data,
      )
    }
    
    // Extrai o nome do arquivo do header Content-Disposition, se disponível
    const contentDisposition = response.headers.get('Content-Disposition')
    let downloadFileName = fileName ?? 'backup.sql.gz'
    
    if (contentDisposition) {
      const match = contentDisposition.match(/filename="?([^"]+)"?/)
      if (match?.[1]) {
        downloadFileName = match[1]
      }
    }
    
    // Criar blob e iniciar download
    const blob = await response.blob()
    const blobUrl = window.URL.createObjectURL(blob)
    
    const link = document.createElement('a')
    link.href = blobUrl
    link.download = downloadFileName
    document.body.appendChild(link)
    link.click()
    
    // Cleanup
    document.body.removeChild(link)
    window.URL.revokeObjectURL(blobUrl)
  },
}

/**
 * Serviço de API para estatísticas
 */
export const statsApi = {
  /**
   * Obtém estatísticas do dashboard
   */
  async get (): Promise<ApiResponse<DashboardStats>> {
    return request<ApiResponse<DashboardStats>>('/stats')
  },
}

/**
 * Serviço de API para logs de auditoria
 */
export const auditLogsApi = {
  /**
   * Lista logs de auditoria com filtros e paginação
   */
  async list (params?: {
    page?: number
    limit?: number
    action?: AuditAction
    entityType?: AuditEntityType
    entityId?: number
    status?: AuditStatus
    startDate?: string
    endDate?: string
  }): Promise<{ success: boolean, data: AuditLog[], meta: { total: number, perPage: number, currentPage: number, lastPage: number } }> {
    const searchParams = new URLSearchParams()

    if (params?.page) {
      searchParams.set('page', params.page.toString())
    }
    if (params?.limit) {
      searchParams.set('limit', params.limit.toString())
    }
    if (params?.action) {
      searchParams.set('action', params.action)
    }
    if (params?.entityType) {
      searchParams.set('entityType', params.entityType)
    }
    if (params?.entityId) {
      searchParams.set('entityId', params.entityId.toString())
    }
    if (params?.status) {
      searchParams.set('status', params.status)
    }
    if (params?.startDate) {
      searchParams.set('startDate', params.startDate)
    }
    if (params?.endDate) {
      searchParams.set('endDate', params.endDate)
    }

    const query = searchParams.toString()
    return request(`/audit-logs${query ? `?${query}` : ''}`)
  },

  /**
   * Obtém um log de auditoria específico
   */
  async get (id: number): Promise<ApiResponse<AuditLog>> {
    return request<ApiResponse<AuditLog>>(`/audit-logs/${id}`)
  },

  /**
   * Obtém estatísticas de auditoria
   */
  async stats (): Promise<ApiResponse<AuditStats>> {
    return request<ApiResponse<AuditStats>>('/audit-logs/stats')
  },
}

/**
 * Serviço de API para Gerenciamento de Usuários
 */
export const usersApi = {
  /**
   * Lista usuários com paginação e filtros
   */
  async list (params?: {
    page?: number
    limit?: number
    active?: boolean | string
  }): Promise<PaginatedResponse<User>> {
    const searchParams = new URLSearchParams()
    if (params?.page) {
      searchParams.set('page', params.page.toString())
    }
    if (params?.limit) {
      searchParams.set('limit', params.limit.toString())
    }
    if (params?.active !== undefined) {
      searchParams.set('active', String(params.active))
    }

    const query = searchParams.toString()
    return request<PaginatedResponse<User>>(`/users${query ? `?${query}` : ''}`)
  },

  /**
   * Alterna status do usuário (aprovar/desativar)
   */
  async toggleStatus (id: number): Promise<ApiResponse<any>> {
    return request<ApiResponse<any>>(`/users/${id}/status`, {
      method: 'PATCH',
    })
  },
}

/**
 * Serviço de API para Autenticação
 */
export const authApi = {
  /**
   * Realiza login
   */
  async login (payload: LoginPayload): Promise<ApiResponse<AuthResponse>> {
    return request<ApiResponse<AuthResponse>>('/auth/login', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  /**
   * Realiza registro
   */
  async register (payload: RegisterPayload): Promise<ApiResponse<AuthResponse>> {
    return request<ApiResponse<AuthResponse>>('/auth/register', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  /**
   * Obtém usuário atual
   */
  async me (): Promise<ApiResponse<any>> {
    return request<ApiResponse<any>>('/auth/me')
  },

  /**
   * Realiza logout
   */
  async logout (): Promise<ApiResponse> {
    return request<ApiResponse>('/auth/logout', {
      method: 'POST',
    })
  },

  /**
   * Verifica se existem usuários cadastrados no sistema
   */
  async checkStatus (): Promise<ApiResponse<{ hasUsers: boolean }>> {
    return request<ApiResponse<{ hasUsers: boolean }>>('/auth/status')
  },
}

/**
 * Health check da API
 */
export async function healthCheck (): Promise<{
  status: string
  timestamp: string
  version: string
}> {
  return request('/health')
}
