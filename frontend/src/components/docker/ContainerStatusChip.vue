<template>
  <v-chip :color="color" label :size="size" variant="tonal">
    <v-icon :icon="icon" start size="14" />
    {{ label }}
  </v-chip>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import type { DockerContainerState } from '@/types/api'

const props = withDefaults(
  defineProps<{
    state: DockerContainerState
    size?: 'x-small' | 'small' | 'default'
  }>(),
  { size: 'small' }
)

const STATE_MAP: Record<string, { color: string; icon: string; label: string }> = {
  running:    { color: 'success', icon: 'mdi-check-circle-outline', label: 'Running' },
  paused:     { color: 'warning', icon: 'mdi-pause-circle-outline', label: 'Paused' },
  restarting: { color: 'info',    icon: 'mdi-restart',              label: 'Restarting' },
  created:    { color: 'info',    icon: 'mdi-plus-circle-outline',  label: 'Created' },
  exited:     { color: 'error',   icon: 'mdi-stop-circle-outline',  label: 'Exited' },
  stopped:    { color: 'error',   icon: 'mdi-stop-circle-outline',  label: 'Stopped' },
  dead:       { color: 'error',   icon: 'mdi-skull-outline',        label: 'Dead' },
}

const resolved = computed(() => STATE_MAP[props.state] ?? { color: 'default', icon: 'mdi-help-circle-outline', label: props.state })
const color = computed(() => resolved.value.color)
const icon  = computed(() => resolved.value.icon)
const label = computed(() => resolved.value.label)
</script>
