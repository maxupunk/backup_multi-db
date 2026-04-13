<template>
  <v-expansion-panels v-model="open" class="mb-4">
    <v-expansion-panel>
      <v-expansion-panel-title class="font-weight-bold">
        <v-icon class="mr-2" icon="mdi-docker" size="20" />
        {{ group.projectName === '_standalone' ? 'Standalone' : group.projectName }}
        <v-chip class="ml-2" label size="x-small" variant="tonal">
          {{ group.containers.length }}
        </v-chip>
        <v-spacer />
        <div class="d-flex ga-1 mr-2" @click.stop>
          <v-btn
            density="compact"
            :disabled="loading"
            size="small"
            variant="tonal"
            @click="emit('stopAll', ids)"
          >
            <v-icon icon="mdi-stop" start />
            Parar todos
          </v-btn>
          <v-btn
            density="compact"
            :disabled="loading"
            size="small"
            variant="tonal"
            @click="emit('restartAll', ids)"
          >
            <v-icon icon="mdi-restart" start />
            Reiniciar todos
          </v-btn>
        </div>
      </v-expansion-panel-title>

      <v-expansion-panel-text>
        <v-row dense>
          <v-col
            v-for="container in group.containers"
            :key="container.id"
            cols="12"
            md="4"
            sm="6"
          >
            <ContainerCard
              :container="container"
              :loading="loading"
              :resources="resourcesById?.[container.id] ?? null"
              @restart="emit('restart', $event)"
              @start="emit('start', $event)"
              @stop="emit('stop', $event)"
            />
          </v-col>
        </v-row>
      </v-expansion-panel-text>
    </v-expansion-panel>
  </v-expansion-panels>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import type { DockerContainerGroup, DockerContainerResourceMetrics } from '@/types/api'
import ContainerCard from './ContainerCard.vue'

const props = defineProps<{
  group: DockerContainerGroup
  loading?: boolean
  resourcesById?: Record<string, DockerContainerResourceMetrics>
}>()

const emit = defineEmits<{
  (e: 'start', id: string): void
  (e: 'stop', id: string): void
  (e: 'restart', id: string): void
  (e: 'stopAll', ids: string[]): void
  (e: 'restartAll', ids: string[]): void
}>()

const open = ref([0])
const ids = computed(() => props.group.containers.map((c) => c.id))
</script>
