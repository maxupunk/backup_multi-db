<template>
  <v-dialog v-model="model" max-width="600" scrollable>
    <v-card>
      <v-card-title class="d-flex align-center pa-4">
        <v-icon class="mr-2" icon="mdi-database-outline" />
        {{ detail?.name }}
      </v-card-title>
      <v-divider />

      <v-card-text class="pa-4" style="max-height: 480px">
        <v-list density="compact" lines="two">
          <v-list-item subtitle="Driver" :title="detail?.driver" />
          <v-list-item subtitle="Escopo" :title="detail?.scope" />
          <v-list-item subtitle="Mountpoint" :title="detail?.mountpoint" />
          <v-list-item v-if="detail?.createdAt" subtitle="Criado em" :title="detail.createdAt" />
        </v-list>

        <template v-if="hasOptions">
          <v-divider class="my-3" />
          <div class="text-caption font-weight-bold mb-2">Opções</div>
          <v-table density="compact">
            <tbody>
              <tr v-for="(val, key) in detail!.options" :key="key">
                <td class="text-caption font-weight-medium">{{ key }}</td>
                <td class="text-caption">{{ val }}</td>
              </tr>
            </tbody>
          </v-table>
        </template>

        <template v-if="hasLabels">
          <v-divider class="my-3" />
          <div class="text-caption font-weight-bold mb-2">Labels</div>
          <v-table density="compact">
            <tbody>
              <tr v-for="(val, key) in detail!.labels" :key="key">
                <td class="text-caption font-weight-medium">{{ key }}</td>
                <td class="text-caption">{{ val }}</td>
              </tr>
            </tbody>
          </v-table>
        </template>
      </v-card-text>

      <v-divider />
      <v-card-actions class="justify-end pa-3">
        <v-btn variant="text" @click="model = false">Fechar</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import type { DockerVolumeDetail } from '@/types/api'

const model = defineModel<boolean>({ default: false })
const props = defineProps<{ detail: DockerVolumeDetail | null }>()

const hasOptions = computed(() =>
  !!props.detail?.options && Object.keys(props.detail.options).length > 0
)
const hasLabels = computed(() =>
  !!props.detail?.labels && Object.keys(props.detail.labels).length > 0
)
</script>
