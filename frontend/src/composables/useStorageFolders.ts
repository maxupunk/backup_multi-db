import { ref } from 'vue'
import { storagesApi } from '@/services/api'

export function useStorageFolders () {
  const folders = ref<string[]>([])
  const loading = ref(false)

  async function loadFolders (storageId: number): Promise<void> {
    loading.value = true
    folders.value = []
    try {
      const response = await storagesApi.browse(storageId, '')
      const objects = response.data?.objects ?? []
      folders.value = objects
        .filter((o) => o.isDirectory)
        .map((o) => o.key)
    } catch {
      // silent — user can still type a path manually
    } finally {
      loading.value = false
    }
  }

  function reset (): void {
    folders.value = []
  }

  return { folders, loading, loadFolders, reset }
}
