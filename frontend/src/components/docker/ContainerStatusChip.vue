<template>
  <v-chip :color="meta.color" label :size="size" variant="tonal">
    <v-icon :icon="meta.icon" size="14" start />
    {{ meta.label }}
  </v-chip>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import type { DockerContainerState } from '@/types/api'
import { resolveContainerStateMeta } from '@/utils/dockerMetrics'

const props = withDefaults(
  defineProps<{
    state: DockerContainerState | string
    size?: 'x-small' | 'small' | 'default'
  }>(),
  { size: 'small' }
)

const meta = computed(() => resolveContainerStateMeta(props.state))
</script>
