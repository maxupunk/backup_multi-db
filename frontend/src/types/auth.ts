import type { User as UserDto } from '@/bindings/User'

/**
 * Interface do Usuário
 */
export type User = UserDto

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
