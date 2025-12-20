import type { User } from '@/types/auth'
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { authApi } from '@/services/api'

export const useAuthStore = defineStore('auth', () => {
  const user = ref<User | null>(null)
  const token = ref<string | null>(localStorage.getItem('token'))
  const isAuthenticated = computed(() => !!token.value)

  /**
   * Inicializa a store, carregando o usuário se tiver token
   */
  async function init () {
    if (token.value) {
      try {
        const response = await authApi.me()
        user.value = response.data.data
      } catch {
        // Se falhar (token inválido/expirado), limpa tudo
        logout()
      }
    }
  }

  /**
   * Define o token e persiste no localStorage
   */
  function setToken (value: string) {
    token.value = value
    localStorage.setItem('token', value)
  }

  /**
   * Define o usuário
   */
  function setUser (value: User) {
    user.value = value
  }

  /**
   * Realiza Logout
   */
  async function logout () {
    try {
      if (token.value) {
        await authApi.logout()
      }
    } catch (error) {
      console.error('Erro ao realizar logout no backend', error)
    } finally {
      token.value = null
      user.value = null
      localStorage.removeItem('token')
      // router.push('/login') // Idealmente feito no componente ou router guard
    }
  }

  return {
    user,
    token,
    isAuthenticated,
    init,
    setToken,
    setUser,
    logout,
  }
})
