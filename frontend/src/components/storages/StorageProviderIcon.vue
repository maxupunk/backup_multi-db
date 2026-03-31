<template>
  <v-icon :color="color" :icon="icon" v-bind="$attrs" />
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import type { StorageProvider } from '@/types/api'
import { getProviderColor, getProviderIcon } from '@/ui/storage'

const props = defineProps<{
  provider: StorageProvider | null | undefined
}>()

const icon = computed(() => getProviderIcon((props.provider ?? 'local') as StorageProvider))
const color = computed(() => getProviderColor((props.provider ?? 'local') as StorageProvider))
</script>
