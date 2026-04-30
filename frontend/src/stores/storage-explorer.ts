import type { BucketObject } from '@/types/api'
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { storagesApi } from '@/services/api'

export interface PathSegment {
  label: string
  path: string
}

export const useStorageExplorerStore = defineStore('storage-explorer', () => {
  const storageId = ref<number | null>(null)
  const currentPath = ref('')
  const objects = ref<BucketObject[]>([])
  const cursor = ref<string | null>(null)
  const loading = ref(false)
  const hasMore = computed(() => cursor.value !== null)

  const breadcrumbs = computed<PathSegment[]>(() => {
    const segments: PathSegment[] = [{ label: 'Raiz', path: '' }]
    if (!currentPath.value) return segments

    const parts = currentPath.value.replace(/\/$/, '').split('/')
    let accumulated = ''
    for (const part of parts) {
      accumulated += part + '/'
      segments.push({ label: part, path: accumulated })
    }
    return segments
  })

  async function browse (id: number, path?: string, appendCursor?: string) {
    storageId.value = id
    loading.value = true

    if (!appendCursor) {
      currentPath.value = path ?? ''
      objects.value = []
      cursor.value = null
    }

    try {
      const response = await storagesApi.browse(id, path ?? '', appendCursor)
      const data = response.data
      if (data) {
        if (appendCursor) {
          objects.value = [...objects.value, ...data.objects]
        } else {
          objects.value = data.objects
        }
        cursor.value = data.cursor
      }
    } finally {
      loading.value = false
    }
  }

  async function removeObject (key: string, isDirectory: boolean) {
    if (storageId.value === null) {
      throw new Error('Nenhum armazenamento selecionado')
    }

    await storagesApi.deleteObject(storageId.value, { key, isDirectory })
    await browse(storageId.value, currentPath.value)
  }

  function navigateTo (path: string) {
    if (storageId.value !== null) {
      browse(storageId.value, path)
    }
  }

  function navigateUp () {
    const parts = currentPath.value.replace(/\/$/, '').split('/')
    parts.pop()
    const parentPath = parts.length > 0 ? parts.join('/') + '/' : ''
    navigateTo(parentPath)
  }

  function loadMore () {
    if (storageId.value !== null && cursor.value) {
      browse(storageId.value, currentPath.value, cursor.value)
    }
  }

  function refresh () {
    if (storageId.value !== null) {
      browse(storageId.value, currentPath.value)
    }
  }

  function reset () {
    storageId.value = null
    currentPath.value = ''
    objects.value = []
    cursor.value = null
    loading.value = false
  }

  return {
    storageId,
    currentPath,
    objects,
    cursor,
    loading,
    hasMore,
    breadcrumbs,
    browse,
    removeObject,
    navigateTo,
    navigateUp,
    loadMore,
    refresh,
    reset,
  }
})
