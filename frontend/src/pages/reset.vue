<template>
  <v-card class="mx-auto pa-4" elevation="4" rounded="lg" width="400">
    <div class="text-center mb-6">
      <v-avatar class="mb-4 elevation-2" color="primary" size="64">
        <v-icon icon="mdi-lock-check" size="32" />
      </v-avatar>
      <h2 class="text-h5 font-weight-bold mb-1">Nova senha</h2>
      <div class="text-subtitle-2 text-medium-emphasis">
        Escolha uma senha de 8 a 32 caracteres.
      </div>
    </div>

    <v-alert v-if="!token" class="mb-4" type="error" variant="tonal">
      Link inválido: falta o token de redefinição. Peça um novo link.
    </v-alert>

    <v-alert v-else-if="done" class="mb-4" type="success" variant="tonal">
      Senha redefinida. Você já pode entrar com a senha nova.
    </v-alert>

    <v-form v-else @submit.prevent="handleSubmit">
      <v-text-field
        v-model="password"
        autofocus
        class="mb-2"
        :error-messages="fieldError"
        label="Nova senha"
        prepend-inner-icon="mdi-lock"
        type="password"
        variant="outlined"
      />

      <v-text-field
        v-model="confirmation"
        class="mb-2"
        :error-messages="confirmationError"
        label="Repita a nova senha"
        prepend-inner-icon="mdi-lock-check"
        type="password"
        variant="outlined"
      />

      <v-alert v-if="errorMessage" class="mb-4" closable type="error" variant="tonal">
        {{ errorMessage }}
      </v-alert>

      <v-btn block class="mb-4" color="primary" :loading="loading" size="large" type="submit">
        Redefinir senha
      </v-btn>
    </v-form>

    <div class="text-center">
      <router-link class="text-decoration-none text-body-2" to="/login">
        Ir para o login
      </router-link>
    </div>
  </v-card>
</template>

<script lang="ts" setup>
  import { computed, ref } from 'vue'
  import { useRoute } from 'vue-router'
  import { ApiError, authApi } from '@/services/api'

  definePage({
    meta: {
      layout: 'authentication',
      public: true,
    },
  })

  const route = useRoute()

  /** O token chega na query string do link enviado por e-mail. */
  const token = computed(() => {
    const value = route.query.token
    return typeof value === 'string' ? value : ''
  })

  const password = ref('')
  const confirmation = ref('')
  const loading = ref(false)
  const done = ref(false)
  const errorMessage = ref('')
  const fieldError = ref('')
  const confirmationError = ref('')

  async function handleSubmit () {
    errorMessage.value = ''
    fieldError.value = ''
    confirmationError.value = ''

    // Conferência local só para evitar uma ida ao servidor por erro de
    // digitação; a regra de tamanho quem decide é o backend.
    if (password.value !== confirmation.value) {
      confirmationError.value = 'As senhas não conferem'
      return
    }

    loading.value = true

    try {
      await authApi.resetPassword(token.value, password.value)
      done.value = true
    } catch (error) {
      if (error instanceof ApiError && error.statusCode === 422) {
        fieldError.value = 'A senha precisa ter entre 8 e 32 caracteres'
      } else if (error instanceof ApiError && error.statusCode === 400) {
        errorMessage.value = 'Link inválido ou expirado. Peça uma nova redefinição.'
      } else if (error instanceof ApiError && error.statusCode === 429) {
        errorMessage.value = 'Muitas tentativas. Aguarde alguns minutos e tente novamente.'
      } else {
        errorMessage.value = 'Não foi possível redefinir a senha. Tente novamente.'
      }
    } finally {
      loading.value = false
    }
  }
</script>
