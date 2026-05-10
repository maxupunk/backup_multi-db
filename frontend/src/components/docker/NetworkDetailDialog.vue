<template>
  <v-dialog v-model="model" max-width="640" scrollable>
    <v-card>
      <v-card-title class="d-flex align-center pa-4">
        <v-icon class="mr-2" icon="mdi-graph-outline" />
        {{ detail?.name }}
        <v-spacer />
        <v-btn
          color="primary"
          prepend-icon="mdi-stethoscope"
          size="small"
          variant="tonal"
          @click="openDiagnostic()"
        >
          Diagnóstico
        </v-btn>
      </v-card-title>
      <v-divider />

      <v-card-text class="pa-4" style="max-height: 520px">
        <v-list density="compact" lines="two">
          <v-list-item subtitle="Driver" :title="detail?.driver" />
          <v-list-item subtitle="Escopo" :title="detail?.scope" />
          <v-list-item subtitle="IPAM Driver" :title="detail?.ipam.driver" />
          <v-list-item
            v-if="detail?.ipam.config.length"
            subtitle="Subnet"
            :title="detail.ipam.config.map((c) => c.subnet).join(', ')"
          />
          <v-list-item subtitle="Interno" :title="detail?.internal ? 'Sim' : 'Não'" />
        </v-list>

        <template v-if="containerEntries.length">
          <v-divider class="my-3" />
          <div class="text-caption font-weight-bold mb-2">Containers conectados</div>
          <v-table density="compact">
            <thead>
              <tr>
                <th>Nome</th>
                <th>IPv4</th>
                <th>MAC</th>
                <th class="text-right">Ações</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="c in containerEntries" :key="c.containerId">
                <td class="text-caption">{{ c.name }}</td>
                <td class="text-caption text-monospace">{{ c.ipv4Address }}</td>
                <td class="text-caption text-monospace">{{ c.macAddress }}</td>
                <td class="text-right">
                  <v-tooltip text="Abrir diagnóstico com alvo do container">
                    <template #activator="{ props: tooltipProps }">
                      <v-btn
                        v-bind="tooltipProps"
                        density="compact"
                        icon="mdi-pulse"
                        size="x-small"
                        variant="text"
                        @click="openDiagnostic(c)"
                      />
                    </template>
                  </v-tooltip>
                </td>
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

  <DockerDiagnosticDialog v-model="diagnosticDialog" :preset="diagnosticPreset" />
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import DockerDiagnosticDialog from '@/components/docker/DockerDiagnosticDialog.vue'
import type {
  DockerDiagnosticPreset,
  DockerDiagnosticTargetOption,
  DockerNetworkContainer,
  DockerNetworkDetail,
} from '@/types/api'

const model = defineModel<boolean>({ default: false })
const props = defineProps<{ detail: DockerNetworkDetail | null }>()

const containerEntries = computed(() => Object.values(props.detail?.containers ?? {}))
const diagnosticDialog = ref(false)
const diagnosticPreset = ref<DockerDiagnosticPreset | null>(null)

function normalizeIpv4Address(value: string): string {
  return value.split('/')[0] ?? value
}

function buildTargetOptions(container?: DockerNetworkContainer): DockerDiagnosticTargetOption[] {
  if (!container) {
    return []
  }

  const options: DockerDiagnosticTargetOption[] = []
  const normalizedIp = normalizeIpv4Address(container.ipv4Address)

  if (container.name) {
    options.push({
      label: `Domínio: ${container.name}`,
      value: container.name,
    })
  }

  if (normalizedIp) {
    options.push({
      label: `IP: ${normalizedIp}`,
      value: normalizedIp,
    })
  }

  return options
}

function openDiagnostic(container?: DockerNetworkContainer) {
  const suggestedTargets = buildTargetOptions(container)

  diagnosticPreset.value = {
    tool: 'ping',
    target: suggestedTargets[0]?.value ?? '',
    contextLabel: container?.name ?? props.detail?.name ?? 'Rede Docker',
    suggestedTargets,
  }

  diagnosticDialog.value = true
}
</script>
