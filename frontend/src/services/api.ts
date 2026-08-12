/**
 * Serviço de API - Cliente HTTP para comunicação com o backend
 */

import type {
  ApiErrorBody,
  MessageResponse,
  ArchiveJob,
  AuditAction,
  AuditEntityType,
  AuditLog,
  AuditStats,
  AuditStatus,
  Backup,
  BackupRetentionPolicySettings,
  BackupRetentionRunResult,
  BackupResult,
  BrowseResult,
  Connection,
  ConnectionTestResult,
  CopyJob,
  CopyStoragePayload,
  CreateConnectionPayload,
  CreateStorageDestinationPayload,
  CreateStoragePayload,
  DeleteStorageObjectPayload,
  DashboardStats,
  DockerContainerResourceOverview,
  DockerHostsResponseData,
  ImportBackupResult,
  LoginPayload,
  Paginated,
  RegisterPayload,
  ResourceMetricsHistoryResponse,
  RestoreOptions,
  RestoreResult,
  Storage,
  StorageDestination,
  StorageProvider,
  StorageSpaceInfo,
  DiagnosticsListing,
  SystemStatus,
  UpdateBackupRetentionPolicyPayload,
  UpdateConnectionPayload,
  UpdateStorageDestinationPayload,
  UpdateStoragePayload,
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
 * Extrai a mensagem exibível de um corpo de erro.
 *
 * O backend tem um formato só — `{ error, description }` —, com `errors` no
 * lugar dos dois quando a falha é de validação. A ordem abaixo é a de utilidade
 * para quem lê: a mensagem do primeiro campo inválido diz mais do que
 * "requisição inválida", e `error` sozinho é um identificador de máquina.
 */
function extractErrorMessage(data: unknown): string {
  if (typeof data !== 'object' || data === null) {
    return 'Erro na requisição'
  }

  const obj = data as Record<string, unknown>

  if (typeof obj.errors === 'object' && obj.errors !== null) {
    const first = Object.values(obj.errors as Record<string, unknown>)[0]
    if (Array.isArray(first) && first.length > 0) {
      const failure = first[0] as { message?: unknown }
      if (typeof failure.message === 'string') {
        return failure.message
      }
    }
  }

  if (typeof obj.description === 'string') {
    return obj.description
  }

  // `message` é o corpo de sucesso das rotas sem recurso; um 4xx dificilmente
  // chega aqui com ele, mas custa uma linha aceitar.
  if (typeof obj.message === 'string') {
    return obj.message
  }

  if (typeof obj.error === 'string') {
    return obj.error
  }

  return 'Erro na requisição'
}

/**
 * Mensagem do primeiro problema de `field`, num corpo de erro de validação.
 *
 * Devolve string vazia quando o campo passou — é o valor que os formulários
 * atribuem ao slot de erro para limpá-lo, então quem chama não precisa
 * distinguir "sem erro" de "campo ausente na resposta".
 */
export function fieldError (body: unknown, field: string): string {
  const errors = (body as ApiErrorBody | undefined)?.errors?.[field]

  return errors?.[0]?.message ?? ''
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
  }): Promise<Paginated<Connection>> {
    const searchParams = new URLSearchParams()

    if (params?.page) {
      searchParams.set('page', params.page.toString())
    }
    if (params?.limit) {
      searchParams.set('page_size', params.limit.toString())
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
    return request<Paginated<Connection>>(
      `/connections${query ? `?${query}` : ''}`,
    )
  },

  /**
   * Obtém uma conexão específica
   */
  async get (id: number): Promise<Connection> {
    return request<Connection>(`/connections/${id}`)
  },

  /**
   * Cria uma nova conexão
   */
  async create (
    payload: CreateConnectionPayload,
  ): Promise<Connection> {
    return request<Connection>('/connections', {
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
  ): Promise<Connection> {
    return request<Connection>(`/connections/${id}`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    })
  },

  /**
   * Remove uma conexão
   */
  async delete (id: number): Promise<MessageResponse> {
    return request<MessageResponse>(`/connections/${id}`, {
      method: 'DELETE',
    })
  },

  /**
   * Testa a conexão
   */
  async test (id: number): Promise<ConnectionTestResult> {
    return request<ConnectionTestResult>(
      `/connections/${id}/test`,
      {
        method: 'POST',
      },
    )
  },

  /**
   * Inicia um backup manual
   */
  async backup (id: number): Promise<BackupResult> {
    return request<BackupResult>(`/connections/${id}/backup`, {
      method: 'POST',
    })
  },

  /**
   * Descobre bancos de dados disponíveis com as credenciais fornecidas
   */
  async discoverDatabases (payload: {
    type: string
    host: string
    port: number
    username: string
    password?: string
  }): Promise<{ databases: string[] }> {
    return request<{ databases: string[] }>('/connections/discover-databases', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  async listDockerHosts (): Promise<DockerHostsResponseData> {
    return request<DockerHostsResponseData>('/connections/docker-hosts')
  },

  /**
   * Cria um novo banco de dados na conexão especificada
   */
  async createDatabase (
    connectionId: number,
    databaseName: string,
  ): Promise<{ databaseName: string }> {
    return request<{ databaseName: string }>(
      `/connections/${connectionId}/create-database`,
      {
        method: 'POST',
        body: JSON.stringify({ databaseName }),
      },
    )
  },
}

export const storageDestinationsApi = {
  async list (params?: {
    page?: number
    limit?: number
    type?: string
    status?: string
    search?: string
  }): Promise<Paginated<StorageDestination>> {
    const searchParams = new URLSearchParams()

    if (params?.page) {
      searchParams.set('page', params.page.toString())
    }
    if (params?.limit) {
      searchParams.set('page_size', params.limit.toString())
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
    return request<Paginated<StorageDestination>>(
      `/storage-destinations${query ? `?${query}` : ''}`,
    )
  },

  async get (id: number): Promise<StorageDestination> {
    return request<StorageDestination>(`/storage-destinations/${id}`)
  },

  async create (
    payload: CreateStorageDestinationPayload,
  ): Promise<StorageDestination> {
    return request<StorageDestination>('/storage-destinations', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  async update (
    id: number,
    payload: UpdateStorageDestinationPayload,
  ): Promise<StorageDestination> {
    return request<StorageDestination>(`/storage-destinations/${id}`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    })
  },

  async delete (id: number): Promise<MessageResponse> {
    return request<MessageResponse>(`/storage-destinations/${id}`, {
      method: 'DELETE',
    })
  },

  /**
   * Obtém informações de espaço de todos os destinos
   */
  async spaceAll (): Promise<StorageSpaceInfo[]> {
    return request<StorageSpaceInfo[]>('/storage-destinations-space')
  },

  /**
   * Obtém informações de espaço de um destino específico
   */
  async space (id: number): Promise<StorageSpaceInfo | null> {
    return request<StorageSpaceInfo | null>(`/storage-destinations/${id}/space`)
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
  }): Promise<Paginated<Backup>> {
    const searchParams = new URLSearchParams()

    if (params?.page) {
      searchParams.set('page', params.page.toString())
    }
    if (params?.limit) {
      searchParams.set('page_size', params.limit.toString())
    }
    if (params?.status) {
      searchParams.set('status', params.status)
    }
    if (params?.connectionId) {
      searchParams.set('connectionId', params.connectionId.toString())
    }

    const query = searchParams.toString()
    return request<Paginated<Backup>>(
      `/backups${query ? `?${query}` : ''}`,
    )
  },

  /**
   * Obtém um backup específico
   */
  async get (id: number): Promise<Backup> {
    return request<Backup>(`/backups/${id}`)
  },

  /**
   * Remove um backup
   */
  async delete (id: number): Promise<MessageResponse> {
    return request<MessageResponse>(`/backups/${id}`, {
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

  /**
   * Restaura um backup para o banco de dados
   */
  async restore (id: number, options?: RestoreOptions): Promise<RestoreResult> {
    return request<RestoreResult>(`/backups/${id}/restore`, {
      method: 'POST',
      body: JSON.stringify(options ?? {}),
    })
  },

  /**
   * Importa um arquivo de backup externo para o sistema.
   * Envia multipart/form-data com o arquivo e metadados.
   */
  async import (formData: FormData): Promise<ImportBackupResult> {
    const url = `${API_BASE}/backups/import`
    const token = localStorage.getItem('token')

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        Accept: 'application/json',
        // Não definir Content-Type: o browser seta multipart/form-data com boundary automaticamente
      },
      body: formData,
    })

    const data = await response.json()

    if (!response.ok) {
      throw new ApiError(extractErrorMessage(data), response.status, data)
    }

    return data as ImportBackupResult
  },
}

/**
 * Serviço de API para estatísticas
 */
export const statsApi = {
  /**
   * Obtém estatísticas do dashboard
   */
  async get (): Promise<DashboardStats> {
    return request<DashboardStats>('/stats')
  },
}

export const systemApi = {
  async status (): Promise<SystemStatus> {
    return request<SystemStatus>('/system/status')
  },

  async retentionPolicy (): Promise<BackupRetentionPolicySettings> {
    return request<BackupRetentionPolicySettings>('/system/backup-retention')
  },

  async updateRetentionPolicy (
    payload: UpdateBackupRetentionPolicyPayload,
  ): Promise<BackupRetentionPolicySettings> {
    return request<BackupRetentionPolicySettings>('/system/backup-retention', {
      method: 'PUT',
      body: JSON.stringify(payload),
    })
  },

  async runRetentionNow (): Promise<BackupRetentionRunResult> {
    return request<BackupRetentionRunResult>('/system/backup-retention/run', {
      method: 'POST',
    })
  },

  async diagnostics (): Promise<DiagnosticsListing> {
    return request<DiagnosticsListing>('/system/diagnostics')
  },

  /**
   * Baixa um artefato de diagnóstico.
   *
   * Usa fetch direto (e não o helper `request`) porque a resposta é binária e
   * pode ter centenas de MB — um heap snapshot tem o tamanho do heap do processo.
   */
  async downloadDiagnostic (name: string): Promise<void> {
    const url = `${API_BASE}/system/diagnostics/${encodeURIComponent(name)}/download`
    const token = localStorage.getItem('token')

    const response = await fetch(url, {
      method: 'GET',
      headers: {
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
    })

    if (!response.ok) {
      const data = await response.json().catch(() => ({}))
      throw new ApiError(extractErrorMessage(data), response.status, data)
    }

    const blob = await response.blob()
    const blobUrl = window.URL.createObjectURL(blob)

    const link = document.createElement('a')
    link.href = blobUrl
    link.download = name
    document.body.append(link)
    link.click()

    link.remove()
    window.URL.revokeObjectURL(blobUrl)
  },

  async deleteDiagnostic (name: string): Promise<null> {
    return request<null>(`/system/diagnostics/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    })
  },

  async containerResources (): Promise<DockerContainerResourceOverview> {
    return request<DockerContainerResourceOverview>('/system/containers/resources')
  },

  async resourcesHistory (rangeHours = 24): Promise<ResourceMetricsHistoryResponse> {
    return request<ResourceMetricsHistoryResponse>(
      `/system/resources/history?rangeHours=${encodeURIComponent(String(rangeHours))}`,
    )
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
  }): Promise<Paginated<AuditLog>> {
    const searchParams = new URLSearchParams()

    if (params?.page) {
      searchParams.set('page', params.page.toString())
    }
    if (params?.limit) {
      searchParams.set('page_size', params.limit.toString())
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
  async get (id: number): Promise<AuditLog> {
    return request<AuditLog>(`/audit-logs/${id}`)
  },

  /**
   * Obtém estatísticas de auditoria
   */
  async stats (): Promise<AuditStats> {
    return request<AuditStats>('/audit-logs/stats')
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
  }): Promise<Paginated<User>> {
    const searchParams = new URLSearchParams()
    if (params?.page) {
      searchParams.set('page', params.page.toString())
    }
    if (params?.limit) {
      searchParams.set('page_size', params.limit.toString())
    }
    if (params?.active !== undefined) {
      searchParams.set('active', String(params.active))
    }

    const query = searchParams.toString()
    return request<Paginated<User>>(`/users${query ? `?${query}` : ''}`)
  },

  /**
   * Alterna status do usuário (aprovar/desativar)
   */
  async toggleStatus (id: number): Promise<any> {
    return request<any>(`/users/${id}/status`, {
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
  async login (payload: LoginPayload): Promise<AuthResponse> {
    return request<AuthResponse>('/auth/login', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  /**
   * Realiza registro
   */
  /**
   * Cadastra uma conta.
   *
   * Dois desfechos, ambos 201: o **primeiro** cadastro nasce administrador
   * ativo e recebe token; os seguintes nascem pendentes e recebem só a
   * mensagem. Por isso o retorno é uma união — quem chama decide pelo `token`.
   */
  async register (payload: RegisterPayload): Promise<AuthResponse | MessageResponse> {
    return request<AuthResponse | MessageResponse>('/auth/register', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  /**
   * Obtém usuário atual
   */
  async me (): Promise<any> {
    return request<any>('/auth/me')
  },

  /**
   * Realiza logout
   */
  async logout (): Promise<MessageResponse> {
    return request<MessageResponse>('/auth/logout', {
      method: 'POST',
    })
  },

  /**
   * Verifica se existem usuários cadastrados no sistema
   */
  async checkStatus (): Promise<{ hasUsers: boolean, requiresBootstrapToken: boolean }> {
    return request<{ hasUsers: boolean, requiresBootstrapToken: boolean }>('/auth/status')
  },

  /**
   * Dispara o envio do e-mail de redefinição de senha.
   *
   * Responde sucesso mesmo para um e-mail não cadastrado — de propósito, para
   * não transformar a tela num diretório de quem tem conta. A interface não
   * pode prometer que o e-mail foi enviado.
   */
  async forgotPassword (email: string): Promise<MessageResponse> {
    return request<MessageResponse>('/auth/forgot', {
      method: 'POST',
      body: JSON.stringify({ email }),
    })
  },

  /**
   * Conclui a redefinição com o token recebido por e-mail.
   */
  async resetPassword (token: string, password: string): Promise<MessageResponse> {
    return request<MessageResponse>('/auth/reset', {
      method: 'POST',
      body: JSON.stringify({ token, password }),
    })
  },
}

/**
 * Serviço de API para Armazenamentos
 */
export const storagesApi = {
  async list (params?: {
    page?: number
    limit?: number
    type?: string
    provider?: StorageProvider
    status?: string
    search?: string
  }): Promise<Paginated<Storage>> {
    const searchParams = new URLSearchParams()

    if (params?.page) searchParams.set('page', params.page.toString())
    if (params?.limit) searchParams.set('page_size', params.limit.toString())
    if (params?.type) searchParams.set('type', params.type)
    if (params?.provider) searchParams.set('provider', params.provider)
    if (params?.status) searchParams.set('status', params.status)
    if (params?.search) searchParams.set('search', params.search)

    const query = searchParams.toString()
    return request<Paginated<Storage>>(
      `/storages${query ? `?${query}` : ''}`,
    )
  },

  async get (id: number): Promise<Storage> {
    return request<Storage>(`/storages/${id}`)
  },

  async create (payload: CreateStoragePayload): Promise<Storage> {
    return request<Storage>('/storages', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  async update (id: number, payload: UpdateStoragePayload): Promise<Storage> {
    return request<Storage>(`/storages/${id}`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    })
  },

  async delete (id: number): Promise<MessageResponse> {
    return request<MessageResponse>(`/storages/${id}`, {
      method: 'DELETE',
    })
  },

  async test (id: number): Promise<{ latencyMs: number }> {
    return request<{ latencyMs: number }>(`/storages/${id}/test`, {
      method: 'POST',
    })
  },

  async browse (id: number, path?: string, cursor?: string): Promise<BrowseResult> {
    const searchParams = new URLSearchParams()
    if (path) searchParams.set('path', path)
    if (cursor) searchParams.set('cursor', cursor)
    const query = searchParams.toString()
    return request<BrowseResult>(
      `/storages/${id}/browse${query ? `?${query}` : ''}`,
    )
  },

  async deleteObject (id: number, payload: DeleteStorageObjectPayload): Promise<MessageResponse> {
    return request<MessageResponse>(`/storages/${id}/object`, {
      method: 'DELETE',
      body: JSON.stringify(payload),
    })
  },

  async startCopy (id: number, payload: CopyStoragePayload): Promise<{ jobId: string }> {
    return request<{ jobId: string }>(`/storages/${id}/copy`, {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  async getCopyJob (jobId: string): Promise<CopyJob> {
    return request<CopyJob>(`/storages/copy-jobs/${jobId}`)
  },

  async startArchive (id: number, path?: string): Promise<{ jobId: string }> {
    return request<{ jobId: string }>(`/storages/${id}/archive`, {
      method: 'POST',
      body: JSON.stringify({ path: path || undefined }),
    })
  },

  async downloadArchive (jobId: string): Promise<void> {
    const url = `${API_BASE}/storages/archive-jobs/${jobId}/download`
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

    const contentDisposition = response.headers.get('Content-Disposition')
    let downloadFileName = 'archive.tar.gz'

    if (contentDisposition) {
      const match = contentDisposition.match(/filename="?([^"]+)"?/)
      if (match?.[1]) {
        downloadFileName = match[1]
      }
    }

    const blob = await response.blob()
    const blobUrl = window.URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = blobUrl
    link.download = downloadFileName
    document.body.appendChild(link)
    link.click()
    document.body.removeChild(link)
    window.URL.revokeObjectURL(blobUrl)
  },

  async getArchiveJob (jobId: string): Promise<ArchiveJob> {
    return request<ArchiveJob>(`/storages/archive-jobs/${jobId}`)
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
