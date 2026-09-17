<template>
  <v-card class="process-table-card" variant="outlined">
    <!-- Floating loading indicator with zero vertical layout shift -->
    <v-progress-linear
      v-if="loading"
      class="table-loading-bar"
      color="primary"
      indeterminate
    />

    <div class="table-container">
      <v-table density="compact" fixed-header hover style="max-height: 520px">
        <thead>
          <tr>
            <th
              v-for="(title, idx) in titles"
              :key="title"
              class="text-no-wrap cursor-pointer user-select-none font-weight-bold"
              :class="resolveHeaderClass(idx)"
              @click="$emit('sort', idx)"
            >
              {{ title }}
              <v-icon
                v-if="sortColumnIndex === idx"
                :icon="sortDesc ? 'mdi-arrow-down' : 'mdi-arrow-up'"
                size="small"
              />
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, rIdx) in processes" :key="getRowKey(row, rIdx)">
            <td
              v-for="(cell, cIdx) in row"
              :key="cIdx"
              :class="resolveCellClass(cIdx)"
            >
              <!-- Highlight PID column -->
              <template v-if="isPidColumn(cIdx)">
                <span class="font-weight-bold text-caption text-primary tabular-cell">
                  {{ cell }}
                </span>
              </template>

              <!-- Command / CMD column -->
              <template v-else-if="isCommandColumn(cIdx)">
                <code class="process-command text-caption">{{ cell }}</code>
              </template>

              <!-- CPU / MEM metric columns -->
              <template v-else-if="isMetricColumn(cIdx)">
                <span
                  class="tabular-cell"
                  :class="Number(cell) > 0 ? 'font-weight-bold text-high-emphasis' : 'text-medium-emphasis'"
                >
                  {{ cell }}%
                </span>
              </template>

              <!-- Numeric columns (VSZ, RSS, TIME, etc.) -->
              <template v-else-if="isNumericColumn(cIdx)">
                <span class="text-caption tabular-cell">{{ cell }}</span>
              </template>

              <!-- Default text cell -->
              <template v-else>
                <span class="text-caption">{{ cell }}</span>
              </template>
            </td>
          </tr>

          <tr v-if="processes.length === 0 && !loading">
            <td
              class="text-caption text-medium-emphasis text-center py-6"
              :colspan="Math.max(1, titles.length)"
            >
              {{ emptyText }}
            </td>
          </tr>
        </tbody>
      </v-table>
    </div>

    <v-divider />

    <div class="d-flex align-center justify-space-between px-4 py-2 text-caption text-medium-emphasis">
      <span>
        Total: <strong>{{ processes.length }}</strong>
        <template v-if="totalCount !== undefined && totalCount !== processes.length">
          de <strong>{{ totalCount }}</strong>
        </template>
        processos
      </span>
      <span v-if="lastUpdated">
        Última atualização: {{ lastUpdated }}
      </span>
    </div>
  </v-card>
</template>

<script lang="ts" setup>
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    titles: string[]
    processes: string[][]
    loading?: boolean
    lastUpdated?: string
    sortColumnIndex?: number | null
    sortDesc?: boolean
    totalCount?: number
    emptyText?: string
  }>(),
  {
    loading: false,
    lastUpdated: '',
    sortColumnIndex: null,
    sortDesc: false,
    totalCount: undefined,
    emptyText: 'Nenhum processo encontrado.',
  }
)

defineEmits<{
  (e: 'sort', colIdx: number): void
}>()

const pidColIndex = computed(() => {
  return props.titles.findIndex((t) => t.toUpperCase() === 'PID')
})

function getRowKey(row: string[], index: number): string {
  const pidIdx = pidColIndex.value
  if (pidIdx !== -1 && row[pidIdx]) {
    return `proc-${row[pidIdx]}`
  }
  return `proc-idx-${index}`
}

function isPidColumn(colIdx: number): boolean {
  return (props.titles[colIdx]?.toUpperCase() ?? '') === 'PID'
}

function isCommandColumn(colIdx: number): boolean {
  const colName = props.titles[colIdx]?.toUpperCase() ?? ''
  return colName === 'COMMAND' || colName === 'CMD'
}

function isMetricColumn(colIdx: number): boolean {
  const colName = props.titles[colIdx]?.toUpperCase() ?? ''
  return colName === '%CPU' || colName === '%MEM'
}

function isNumericColumn(colIdx: number): boolean {
  const col = props.titles[colIdx]?.toUpperCase() ?? ''
  return ['VSZ', 'RSS', 'TIME', 'START'].includes(col)
}

function resolveHeaderClass(colIdx: number): string {
  const col = props.titles[colIdx]?.toUpperCase() ?? ''
  if (col === 'PID') return 'col-pid'
  if (col === '%CPU' || col === '%MEM') return 'col-metric'
  if (col === 'VSZ' || col === 'RSS') return 'col-mem-kb'
  if (col === 'COMMAND' || col === 'CMD') return 'col-command'
  return ''
}

function resolveCellClass(colIdx: number): string {
  if (isCommandColumn(colIdx)) {
    return 'command-cell'
  }
  const col = props.titles[colIdx]?.toUpperCase() ?? ''
  if (col === 'PID') return 'text-no-wrap col-pid'
  if (col === '%CPU' || col === '%MEM') return 'text-no-wrap col-metric'
  if (col === 'VSZ' || col === 'RSS') return 'text-no-wrap col-mem-kb'
  return 'text-no-wrap'
}
</script>

<style scoped>
.process-table-card {
  position: relative;
}

.table-loading-bar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  z-index: 3;
  height: 2px !important;
}

.table-container {
  overflow-x: auto;
}

.process-command {
  font-family: 'JetBrains Mono', 'Fira Code', Menlo, Monaco, Consolas, 'Courier New', monospace;
  font-size: 0.75rem;
  background-color: rgba(var(--v-theme-on-surface), 0.05);
  padding: 2px 6px;
  border-radius: 4px;
  display: inline-block;
  max-width: 600px;
  overflow-wrap: break-word;
  white-space: pre-wrap;
  line-height: 1.3;
}

.command-cell {
  max-width: 620px;
}

.cursor-pointer {
  cursor: pointer;
}

.user-select-none {
  user-select: none;
}

.tabular-cell {
  font-variant-numeric: tabular-nums;
  font-feature-settings: "tnum";
}

.col-pid {
  min-width: 65px;
}

.col-metric {
  min-width: 75px;
}

.col-mem-kb {
  min-width: 85px;
}

.col-command {
  min-width: 250px;
}
</style>
