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
      <v-text-field
        v-model="form.email"
        autofocus
        class="mb-2"
        :error-messages="errors.email"
        label="E-mail"
        prepend-inner-icon="mdi-email"
        variant="outlined"
      />

      <v-text-field
        v-model="form.password"
        class="mb-4"
        :error-messages="errors.password"
        label="Senha"
        prepend-inner-icon="mdi-lock"
        type="password"
        variant="outlined"
      />

      <v-alert
        v-if="errorMessage"
        class="mb-4"
        closable
        type="error"
        variant="tonal"
      >
        {{ errorMessage }}
      </v-alert>

      <v-btn
        block
        class="mb-4"
        color="primary"
        :loading="loading"
        size="large"
        type="submit"
      >
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
  import { reactive, ref } from 'vue'
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

  async function handleLogin () {
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
        // Erro de validação
        if (error.statusCode === 422 && error.data && typeof error.data === 'object' && 'errors' in error.data) {
          const validationErrors = (error.data as any).errors
          if (Array.isArray(validationErrors)) {
            for (const err of validationErrors) {
              if (err.field === 'email') errors.email = err.message
              if (err.field === 'password') errors.password = err.message
            }
          }
        } else {
          errorMessage.value = error.message
        }
      } else {
        errorMessage.value = 'Ocorreu um erro ao fazer login. Tente novamente.'
      }
    } finally {
      loading.value = false
    }
  }
</script>
