import type {
  CreateStoragePayload,
  Storage,
  StorageProvider,
  UpdateStoragePayload,
} from '@/types/api'
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { ApiError, storagesApi } from '@/services/api'

export const useStoragesStore = defineStore('storages', () => {
  const storages = ref<Storage[]>([])
  const loading = ref(false)
  const pagination = ref({
    total: 0,
    perPage: 15,
    currentPage: 1,
    lastPage: 1,
  })

  const activeStorages = computed(() =>
    storages.value.filter((s) => s.status === 'active'),
  )

  async function fetchAll (filters?: {
    page?: number
    limit?: number
    type?: string
    provider?: StorageProvider
    status?: string
    search?: string
  }) {
    loading.value = true
    try {
      const response = await storagesApi.list({
        ...filters,
        limit: filters?.limit ?? pagination.value.perPage,
      })
      storages.value = response.data?.data ?? []
      if (response.data?.meta) {
        pagination.value = response.data.meta
      }
    } catch (error) {
      storages.value = []
      throw error
    } finally {
      loading.value = false
    }
  }

  async function create (payload: CreateStoragePayload): Promise<Storage> {
    const response = await storagesApi.create(payload)
    if (!response.data) throw new ApiError('Resposta inválida', 500)
    return response.data
  }

  async function update (id: number, payload: UpdateStoragePayload): Promise<Storage> {
    const response = await storagesApi.update(id, payload)
    if (!response.data) throw new ApiError('Resposta inválida', 500)
    return response.data
  }

  async function remove (id: number) {
    await storagesApi.delete(id)
    storages.value = storages.value.filter((s) => s.id !== id)
  }

  async function testConnection (id: number): Promise<{ latencyMs: number }> {
    const response = await storagesApi.test(id)
    if (!response.data) throw new ApiError('Resposta inválida', 500)
    return response.data
  }

  return {
    storages,
    loading,
    pagination,
    activeStorages,
    fetchAll,
    create,
    update,
    remove,
    testConnection,
  }
})
