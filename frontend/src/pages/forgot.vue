<template>
  <v-card class="mx-auto pa-4" elevation="4" rounded="lg" width="400">
    <div class="text-center mb-6">
      <v-avatar class="mb-4 elevation-2" color="primary" size="64">
        <v-icon icon="mdi-lock-reset" size="32" />
      </v-avatar>
      <h2 class="text-h5 font-weight-bold mb-1">Esqueci minha senha</h2>
      <div class="text-subtitle-2 text-medium-emphasis">
        Informe o e-mail da conta e enviaremos um link de redefinição.
      </div>
    </div>

    <v-alert v-if="sent" class="mb-4" type="success" variant="tonal">
      Se o e-mail estiver cadastrado, você receberá as instruções em instantes.
      O link vale por 4 horas.
    </v-alert>

    <v-form v-else @submit.prevent="handleSubmit">
      <v-text-field
        v-model="email"
        autofocus
        class="mb-2"
        :error-messages="fieldError"
        label="E-mail"
        prepend-inner-icon="mdi-email"
        variant="outlined"
      />

      <v-alert v-if="errorMessage" class="mb-4" closable type="error" variant="tonal">
        {{ errorMessage }}
      </v-alert>

      <v-btn block class="mb-4" color="primary" :loading="loading" size="large" type="submit">
        Enviar link
      </v-btn>
    </v-form>

    <div class="text-center">
      <router-link class="text-decoration-none text-body-2" to="/login">
        Voltar para o login
      </router-link>
    </div>
  </v-card>
</template>

<script lang="ts" setup>
  import { ref } from 'vue'
  import { ApiError, authApi } from '@/services/api'

  definePage({
    meta: {
      layout: 'authentication',
      public: true,
    },
  })

  const email = ref('')
  const loading = ref(false)
  const sent = ref(false)
  const errorMessage = ref('')
  const fieldError = ref('')

  /**
   * O sucesso exibido é o mesmo para e-mail existente e inexistente: o backend
   * responde igual nos dois casos, e a tela não pode revelar a diferença.
   */
  async function handleSubmit () {
    loading.value = true
    errorMessage.value = ''
    fieldError.value = ''

    try {
      await authApi.forgotPassword(email.value)
      sent.value = true
    } catch (error) {
      if (error instanceof ApiError && error.statusCode === 422) {
        fieldError.value = 'Informe um e-mail válido'
      } else if (error instanceof ApiError && error.statusCode === 429) {
        errorMessage.value = 'Muitas tentativas. Aguarde alguns minutos e tente novamente.'
      } else {
        errorMessage.value = 'Não foi possível enviar o link. Tente novamente.'
      }
    } finally {
      loading.value = false
    }
  }
</script>
