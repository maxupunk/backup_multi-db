<template>
    <v-card width="400" class="mx-auto pa-4" elevation="4" rounded="lg">
        <div class="text-center mb-6">
            <v-avatar color="primary" size="64" class="mb-4 elevation-2">
                <v-icon icon="mdi-account-plus" size="32" />
            </v-avatar>
            <h2 class="text-h4 font-weight-bold mb-1">Criar Conta</h2>
            <div class="text-subtitle-1 text-medium-emphasis">Preencha os dados para se registrar</div>
        </div>

        <v-form @submit.prevent="handleRegister">
            <v-text-field v-model="form.fullName" label="Nome Completo" prepend-inner-icon="mdi-account"
                variant="outlined" :error-messages="errors.fullName" class="mb-2" autofocus />

            <v-text-field v-model="form.email" label="E-mail" prepend-inner-icon="mdi-email" variant="outlined"
                :error-messages="errors.email" class="mb-2" />

            <v-text-field v-model="form.password" label="Senha" type="password" prepend-inner-icon="mdi-lock"
                variant="outlined" :error-messages="errors.password" class="mb-4" hint="Mínimo 8 caracteres" />

            <v-alert v-if="errorMessage" type="error" variant="tonal" class="mb-4" closable>
                {{ errorMessage }}
            </v-alert>

            <v-btn type="submit" block color="primary" size="large" :loading="loading" class="mb-4">
                Cadastrar
            </v-btn>

            <div class="text-center">
                <router-link to="/login" class="text-decoration-none text-body-2">
                    Já tem uma conta? Faça login
                </router-link>
            </div>
        </v-form>
    </v-card>
</template>

<script lang="ts" setup>
import { reactive, ref } from 'vue'
import { useAuthStore } from '@/stores/auth'
import { useRouter } from 'vue-router'
import { ApiError } from '@/services/api'
import { authApi } from '@/services/api'

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
    fullName: '',
    email: '',
    password: '',
})
const errors = reactive({
    fullName: '',
    email: '',
    password: '',
})

async function handleRegister() {
    loading.value = true
    errorMessage.value = ''
    errors.fullName = ''
    errors.email = ''
    errors.password = ''

    try {
        const response = await authApi.register(form)
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
                    validationErrors.forEach((err: any) => {
                        if (err.field === 'fullName') errors.fullName = err.message
                        if (err.field === 'email') errors.email = err.message
                        if (err.field === 'password') errors.password = err.message
                    })
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
