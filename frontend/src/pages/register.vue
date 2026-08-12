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

      <v-text-field
        v-if="showBootstrapToken"
        v-model="form.bootstrapToken"
        class="mb-4"
        :error-messages="errors.bootstrapToken"
        hint="Obrigatório para criar o administrador inicial em produção"
        label="Token de Bootstrap"
        prepend-inner-icon="mdi-shield-key"
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
  import { onMounted, reactive, ref } from 'vue'
  import { useRouter } from 'vue-router'
  import { ApiError, authApi, fieldError } from '@/services/api'
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
  const showBootstrapToken = ref(false)
  const form = reactive({
    fullName: '',
    email: '',
    password: '',
    bootstrapToken: '',
  })
  const errors = reactive({
    fullName: '',
    email: '',
    password: '',
    bootstrapToken: '',
  })

  async function handleRegister () {
    loading.value = true
    errorMessage.value = ''
    errors.fullName = ''
    errors.email = ''
    errors.password = ''
    errors.bootstrapToken = ''

    try {
      const response = await authApi.register(form)

      // O primeiro cadastro nasce administrador ativo e vem com token; os
      // seguintes ficam pendentes e trazem só a mensagem.
      if ('token' in response) {
        authStore.setToken(response.token)
        authStore.setUser(response.user)
        router.push('/')
      } else {
        form.fullName = ''
        form.email = ''
        form.password = ''
        form.bootstrapToken = ''
        errorMessage.value = ''
        successMessage.value = response.message
      }
    } catch (error) {
      successMessage.value = ''
      console.error(error)
      if (error instanceof ApiError) {
        // Falha de validação: um problema por campo, sob o nome dele.
        if (error.statusCode === 400) {
          errors.fullName = fieldError(error.data, 'fullName')
          errors.email = fieldError(error.data, 'email')
          errors.password = fieldError(error.data, 'password')
          errors.bootstrapToken = fieldError(error.data, 'bootstrapToken')
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

  onMounted(async () => {
    try {
      const response = await authApi.checkStatus()
      showBootstrapToken.value = !!response && !response.hasUsers
    } catch (error) {
      console.error('Falha ao verificar status do bootstrap:', error)
    }
  })
</script>
