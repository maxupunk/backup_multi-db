<template>
  <div>
    <v-row align="center" class="mb-6">
      <v-col>
        <h1 class="font-weight-bold mb-1 text-h4">Docker Manager</h1>
        <p class="text-body-2 text-medium-emphasis">
          Visão geral do ambiente Docker
        </p>
      </v-col>
      <v-col cols="auto">
        <v-btn :loading="loading" prepend-icon="mdi-refresh" variant="tonal" @click="load">
          Atualizar
        </v-btn>
      </v-col>
    </v-row>

    <DockerUnavailableBanner v-if="unavailable" />

    <v-row v-else>
      <v-col cols="12" md="3" sm="6">
        <v-card to="/docker/containers" variant="outlined">
          <v-card-text class="d-flex align-center ga-4 pa-5">
            <v-avatar color="success" rounded="lg" size="48">
              <v-icon icon="mdi-cube-outline" />
            </v-avatar>
            <div>
              <div class="text-h5 font-weight-bold">{{ running }}</div>
              <div class="text-caption text-medium-emphasis">Containers em execução</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card to="/docker/containers" variant="outlined">
          <v-card-text class="d-flex align-center ga-4 pa-5">
            <v-avatar color="error" rounded="lg" size="48">
              <v-icon icon="mdi-stop-circle-outline" />
            </v-avatar>
            <div>
              <div class="text-h5 font-weight-bold">{{ stopped }}</div>
              <div class="text-caption text-medium-emphasis">Containers parados</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card to="/docker/volumes" variant="outlined">
          <v-card-text class="d-flex align-center ga-4 pa-5">
            <v-avatar color="primary" rounded="lg" size="48">
              <v-icon icon="mdi-database-outline" />
            </v-avatar>
            <div>
              <div class="text-h5 font-weight-bold">{{ volumes }}</div>
              <div class="text-caption text-medium-emphasis">Volumes</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card to="/docker/images" variant="outlined">
          <v-card-text class="d-flex align-center ga-4 pa-5">
            <v-avatar color="info" rounded="lg" size="48">
              <v-icon icon="mdi-layers-outline" />
            </v-avatar>
            <div>
              <div class="text-h5 font-weight-bold">{{ images }}</div>
              <div class="text-caption text-medium-emphasis">Imagens</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue'
import { dockerContainersApi, dockerImagesApi, dockerVolumesApi } from '@/services/dockerService'
import DockerUnavailableBanner from '@/components/docker/DockerUnavailableBanner.vue'
import type { DockerContainerGroup } from '@/types/api'

const loading = ref(false)
const unavailable = ref(false)
const groups = ref<DockerContainerGroup[]>([])
const volumes = ref(0)
const images = ref(0)

const allContainers = computed(() => groups.value.flatMap((g) => g.containers))
const running = computed(() => allContainers.value.filter((c) => c.state === 'running').length)
const stopped = computed(() => allContainers.value.filter((c) => c.state !== 'running').length)

async function load() {
  loading.value = true
  unavailable.value = false
  try {
    const [g, v, imgs] = await Promise.all([
      dockerContainersApi.getGroups(),
      dockerVolumesApi.list(),
      dockerImagesApi.list(),
    ])
    groups.value = g
    volumes.value = v.length
    images.value = imgs.length
  } catch {
    unavailable.value = true
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>
