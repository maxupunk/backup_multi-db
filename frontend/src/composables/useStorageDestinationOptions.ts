import type { ComputedRef, Ref } from 'vue'
import { computed } from 'vue'

import type { StorageDestination } from '@/types/api'

export type StorageDestinationOption = {
  title: string
  value: number
}

export function useStorageDestinationOptions (
  storageDestinations: Ref<StorageDestination[]>,
): ComputedRef<StorageDestinationOption[]> {
  return computed(() =>
    storageDestinations.value.map(d => ({
      title: d.isDefault ? `${d.name} (padrão)` : d.name,
      value: d.id,
    })),
  )
}

