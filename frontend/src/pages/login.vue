<template>
  <v-card class="mx-auto pa-4" elevation="4" rounded="lg" width="400">
    <div class="text-center mb-6">
      <v-avatar class="mb-4 elevation-2" color="primary" size="64">
        <v-icon icon="mdi-database-sync" size="32" />
      </v-avatar>
      <h2 class="text-h4 font-weight-bold mb-1">DB Backup</h2>
      <div class="text-subtitle-1 text-medium-emphasis">Faça login para continuar</div>
    </div>

    <v-form @submit.prevent="handleLogin">
      <v-text-field v-model="form.email" autofocus class="mb-2" :error-messages="errors.email" label="E-mail"
        prepend-inner-icon="mdi-email" variant="outlined" />

      <v-text-field v-model="form.password" class="mb-4" :error-messages="errors.password" label="Senha"
        prepend-inner-icon="mdi-lock" type="password" variant="outlined" />

      <v-alert v-if="errorMessage" class="mb-4" closable type="error" variant="tonal">
        {{ errorMessage }}
      </v-alert>

      <v-btn block class="mb-4" color="primary" :loading="loading" size="large" type="submit">
        Entrar
      </v-btn>

      <div class="text-center">
        <router-link class="text-decoration-none text-body-2" to="/register">
          Não tem uma conta? Cadastre-se
        </router-link>
      </div>
    </v-form>
  </v-card>
</template>

<script lang="ts" setup>
import { onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ApiError, authApi } from '@/services/api'
import { useAuthStore } from '@/stores/auth'

// Define layout authentication
definePage({
  meta: {
    layout: 'authentication',
    public: true,
  },
})

/**
 * Mapa de tradução de mensagens de erro do backend para português
 */
const errorMessages: Record<string, string> = {
  'Invalid user credentials': 'E-mail ou senha incorretos',
  'Invalid credentials': 'E-mail ou senha incorretos',
  'User not found': 'Usuário não encontrado',
  'Account disabled': 'Sua conta está desativada. Entre em contato com o administrador.',
  'Account not approved': 'Sua conta ainda não foi aprovada. Aguarde a aprovação do administrador.',
  'Too many requests': 'Muitas tentativas de login. Aguarde alguns minutos e tente novamente.',
  'Email is required': 'O e-mail é obrigatório',
  'Password is required': 'A senha é obrigatória',
  'Invalid email format': 'Formato de e-mail inválido',
}

/**
 * Traduz mensagem de erro do backend para português
 */
function translateError(message: string): string {
  // Procura por correspondência exata
  if (errorMessages[message]) {
    return errorMessages[message]
  }

  // Procura por correspondência parcial (case insensitive)
  const lowerMessage = message.toLowerCase()
  for (const [key, value] of Object.entries(errorMessages)) {
    if (lowerMessage.includes(key.toLowerCase())) {
      return value
    }
  }

  // Retorna a mensagem original se não houver tradução
  return message
}

const authStore = useAuthStore()
const router = useRouter()

const loading = ref(false)
const errorMessage = ref('')
const form = reactive({
  email: '',
  password: '',
})
const errors = reactive({
  email: '',
  password: '',
})

async function handleLogin() {
  loading.value = true
  errorMessage.value = ''
  errors.email = ''
  errors.password = ''

  try {
    const response = await authApi.login(form)
    if (response.success && response.data) {
      authStore.setToken(response.data.token)
      authStore.setUser(response.data.user)
      router.push('/')
    }
  } catch (error) {
    console.error(error)
    if (error instanceof ApiError) {
      // Erro de validação (422)
      if (error.statusCode === 422 && error.data && typeof error.data === 'object' && 'errors' in error.data) {
        const validationErrors = (error.data as any).errors
        if (Array.isArray(validationErrors)) {
          for (const err of validationErrors) {
            const translatedMessage = translateError(err.message || '')
            if (err.field === 'email') errors.email = translatedMessage
            if (err.field === 'password') errors.password = translatedMessage
          }
        }
      } else {
        // Outros erros da API (401, 400, etc.)
        errorMessage.value = translateError(error.message)
      }
    } else {
      errorMessage.value = 'Ocorreu um erro ao fazer login. Tente novamente.'
    }
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  try {
    const response = await authApi.checkStatus()
    if (response.success && response.data && !response.data.hasUsers) {
      router.push('/register')
    }
  } catch (error) {
    console.error('Failed to check system status:', error)
  }
})
</script>
