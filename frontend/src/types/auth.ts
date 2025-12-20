/**
 * Interface do Usuário
 */
export interface User {
  id: number
  email: string
  fullName: string | null
  isActive?: boolean
  createdAt?: string
  updatedAt?: string
}

/**
 * Resposta de Login/Registro
 */
export interface AuthResponse {
  type: string
  token: string
  user: User
}

/**
 * Estado da Autenticação
 */
export interface AuthState {
  user: User | null
  token: string | null
  isAuthenticated: boolean
}
