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
    page: 1,
    page_size: 15,
    total_pages: 1,
    total_items: 0,
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
        limit: filters?.limit ?? pagination.value.page_size,
      })
      storages.value = response.results
      if (response.pagination) {
        pagination.value = response.pagination
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
    if (!response) throw new ApiError('Resposta inválida', 500)
    return response
  }

  async function update (id: number, payload: UpdateStoragePayload): Promise<Storage> {
    const response = await storagesApi.update(id, payload)
    if (!response) throw new ApiError('Resposta inválida', 500)
    return response
  }

  async function remove (id: number) {
    await storagesApi.delete(id)
    storages.value = storages.value.filter((s) => s.id !== id)
  }

  async function testConnection (id: number): Promise<{ latencyMs: number }> {
    const response = await storagesApi.test(id)
    if (!response) throw new ApiError('Resposta inválida', 500)
    return response
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
