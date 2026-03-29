import type { ComputedRef, Ref } from 'vue'
import { computed } from 'vue'

import type { StorageDestination } from '@/types/api'

export type StorageDestinationOption = {
  title: string
  value: number
}

const DEFAULT_SUFFIX = ' (padrão)'

function formatStorageDestinationTitle(destination: StorageDestination): string {
  if (!destination.isDefault) {
    return destination.name
  }

  return destination.name.endsWith(DEFAULT_SUFFIX)
    ? destination.name
    : `${destination.name}${DEFAULT_SUFFIX}`
}

export function useStorageDestinationOptions (
  storageDestinations: Ref<StorageDestination[]>,
): ComputedRef<StorageDestinationOption[]> {
  return computed(() =>
    storageDestinations.value.map(d => ({
      title: formatStorageDestinationTitle(d),
      value: d.id,
    })),
  )
}
