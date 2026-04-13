<template>
  <v-dialog v-model="model" max-width="640" scrollable>
    <v-card>
      <v-card-title class="d-flex align-center pa-4">
        <v-icon class="mr-2" icon="mdi-layers-outline" />
        {{ detail?.repoTags[0] ?? shortId }}
      </v-card-title>
      <v-divider />

      <v-card-text class="pa-4" style="max-height: 540px">
        <v-list density="compact" lines="two">
          <v-list-item subtitle="ID" :title="shortId" />
          <v-list-item subtitle="Criado" :title="detail?.created" />
          <v-list-item subtitle="Tamanho" :title="formattedSize" />
          <v-list-item
            v-if="detail?.config.workingDir"
            subtitle="Working Dir"
            :title="detail.config.workingDir"
          />
          <v-list-item
            v-if="detail?.config.user"
            subtitle="Usuário"
            :title="detail.config.user"
          />
        </v-list>

        <template v-if="detail?.config.cmd?.length">
          <v-divider class="my-3" />
          <div class="text-caption font-weight-bold mb-1">CMD</div>
          <code class="text-caption d-block pa-2 rounded bg-surface-variant">
            {{ detail.config.cmd.join(' ') }}
          </code>
        </template>

        <template v-if="detail?.config.entrypoint?.length">
          <v-divider class="my-3" />
          <div class="text-caption font-weight-bold mb-1">Entrypoint</div>
          <code class="text-caption d-block pa-2 rounded bg-surface-variant">
            {{ detail.config.entrypoint.join(' ') }}
          </code>
        </template>

        <template v-if="detail?.config.env?.length">
          <v-divider class="my-3" />
          <div class="text-caption font-weight-bold mb-2">Variáveis de Ambiente</div>
          <v-list density="compact">
            <v-list-item
              v-for="env in detail.config.env"
              :key="env"
              class="text-caption text-monospace"
              :title="env"
            />
          </v-list>
        </template>

        <template v-if="detail?.rootFs.layers.length">
          <v-divider class="my-3" />
          <div class="text-caption font-weight-bold mb-2">
            Layers ({{ detail.rootFs.layers.length }})
          </div>
          <v-list density="compact">
            <v-list-item
              v-for="layer in detail.rootFs.layers"
              :key="layer"
              class="text-caption text-monospace"
              :title="layer.replace('sha256:', '').slice(0, 24) + '...'"
            />
          </v-list>
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
import type { DockerImageDetail } from '@/types/api'

const model = defineModel<boolean>({ default: false })
const props = defineProps<{ detail: DockerImageDetail | null }>()

const shortId = computed(() =>
  props.detail ? props.detail.id.replace('sha256:', '').slice(0, 12) : ''
)
const formattedSize = computed(() => {
  if (!props.detail) return ''
  const mb = props.detail.size / 1024 / 1024
  return mb >= 1000 ? `${(mb / 1024).toFixed(1)} GB` : `${mb.toFixed(0)} MB`
})
</script>
