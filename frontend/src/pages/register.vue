<template>
  <v-card class="mx-auto pa-4" elevation="4" rounded="lg" width="400">
    <div class="text-center mb-6">
      <v-avatar class="mb-4 elevation-2" color="primary" size="64">
        <v-icon icon="mdi-account-plus" size="32" />
      </v-avatar>
      <h2 class="text-h4 font-weight-bold mb-1">Criar Conta</h2>
      <div class="text-subtitle-1 text-medium-emphasis">Preencha os dados para se registrar</div>
    </div>

    <v-form @submit.prevent="handleRegister">
      <v-text-field
        v-model="form.fullName"
        autofocus
        class="mb-2"
        :error-messages="errors.fullName"
        label="Nome Completo"
        prepend-inner-icon="mdi-account"
        variant="outlined"
      />

      <v-text-field
        v-model="form.email"
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
        hint="Mínimo 8 caracteres"
        label="Senha"
        prepend-inner-icon="mdi-lock"
        type="password"
        variant="outlined"
      />

      <v-alert
        v-if="successMessage"
        class="mb-4"
        closable
        type="success"
        variant="tonal"
      >
        {{ successMessage }}
      </v-alert>

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
        Cadastrar
      </v-btn>

      <div class="text-center">
        <router-link class="text-decoration-none text-body-2" to="/login">
          Já tem uma conta? Faça login
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
  const successMessage = ref('')
  const form = reactive({
    fullName: '',
    email: '',
    password: '',
  })
  const errors = reactive({
    fullName: '',
    email: '',
    password: '',
  })

  async function handleRegister () {
    loading.value = true
    errorMessage.value = ''
    errors.fullName = ''
    errors.email = ''
    errors.password = ''

    try {
      const response = await authApi.register(form)
      if (response.success) {
        // Se tiver token, login automático
        if (response.data && response.data.token) {
          authStore.setToken(response.data.token)
          authStore.setUser(response.data.user)
          router.push('/')
        } else {
          // Caso contrário pode estar pendente de aprovação
          // Limpar formulário
          form.fullName = ''
          form.email = ''
          form.password = ''
          errorMessage.value = ''

          // Exibir mensagem de sucesso (usando errorMessage como alerta verde ou criando um novo estado)
          // Vamos usar um alert simples ou reaproveitar errorMessage como "success" se adicionar type
          // Por simplicidade, vou usar um alert separado se possível, ou mudar o texto do errorMessage para type success.
          // Vou adicionar successMessage no state.
          successMessage.value = response.message || 'Cadastro realizado. Aguarde aprovação.'
        }
      }
    } catch (error) {
      successMessage.value = ''
      console.error(error)
      if (error instanceof ApiError) {
        // Erro de validação
        if (error.statusCode === 422 && error.data && typeof error.data === 'object' && 'errors' in error.data) {
          const validationErrors = (error.data as any).errors
          if (Array.isArray(validationErrors)) {
            for (const err of validationErrors) {
              if (err.field === 'fullName') errors.fullName = err.message
              if (err.field === 'email') errors.email = err.message
              if (err.field === 'password') errors.password = err.message
            }
          }
        } else {
          errorMessage.value = error.message
        }
      } else {
        errorMessage.value = 'Ocorreu um erro ao criar conta. Tente novamente.'
      }
    } finally {
      loading.value = false
    }
  }
</script>
