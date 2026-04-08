<template>
  <div class="chart-root">
    <!-- Y-axis labels: height matches only the SVG area, not the x-axis -->
    <div class="chart-y-axis" :style="{ height: `${height}px` }">
      <span
        v-for="tick in Y_TICKS"
        :key="tick.value"
        class="y-tick-label"
      >
        {{ tick.label }}
      </span>
    </div>

    <!-- Chart area + X-axis -->
    <div class="chart-column">
      <div
        class="chart-area-wrapper"
        :style="{ height: `${height}px` }"
      >
        <svg
          viewBox="0 0 360 100"
          class="chart-svg"
          preserveAspectRatio="none"
          role="img"
          aria-label="Histórico de uso percentual"
          @mouseleave="clearHoveredPoint"
          @mousemove="handlePointerMove"
        >
          <defs>
            <linearGradient :id="gradientId" x1="0%" y1="0%" x2="0%" y2="100%">
              <stop offset="0%" :stop-color="color" stop-opacity="0.35" />
              <stop offset="100%" :stop-color="color" stop-opacity="0.02" />
            </linearGradient>
          </defs>

          <!-- Horizontal grid lines aligned with Y-axis ticks -->
          <line
            v-for="tick in Y_TICKS"
            :key="tick.value"
            x1="0"
            x2="360"
            :y1="percentToSvgY(tick.value)"
            :y2="percentToSvgY(tick.value)"
            class="grid-line"
          />

          <path
            v-if="areaPath"
            :d="areaPath"
            :fill="`url(#${gradientId})`"
          />

          <polyline
            v-if="linePath"
            :points="linePath"
            :stroke="color"
            class="line-path"
          />

          <circle
            v-if="hoveredPoint"
            :cx="hoveredPoint.x"
            :cy="hoveredPoint.y"
            r="4"
            :fill="color"
            class="hover-dot"
          />
        </svg>

        <div
          v-if="hoveredPoint"
          class="chart-tooltip"
          :style="{ left: `${hoveredPoint.tooltipLeft}px` }"
        >
          <span v-if="hoveredPoint.timeLabel" class="tooltip-time">{{ hoveredPoint.timeLabel }}</span>
          {{ hoveredPoint.value.toFixed(1) }}%
        </div>
      </div>

      <!-- X-axis labels -->
      <div v-if="xTicks.length" class="chart-x-axis">
        <span
          v-for="tick in xTicks"
          :key="tick.index"
          class="x-tick-label"
          :class="tick.position"
          :style="{ left: `${tick.percent}%` }"
        >
          {{ tick.label }}
        </span>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'

// Fixed SVG coordinate space: Y=0 → 100%, Y=100 → 0%
const SVG_W = 360
const SVG_H = 100

const Y_TICKS = [
  { value: 100, label: '100%' },
  { value: 75, label: '75%' },
  { value: 50, label: '50%' },
  { value: 25, label: '25%' },
  { value: 0, label: '0%' },
] as const

const X_TICK_COUNT = 5

const props = withDefaults(defineProps<{
  values: number[]
  timestamps?: string[]
  rangeHours?: number
  color?: string
  height?: number
}>(), {
  color: 'rgb(var(--v-theme-primary))',
  height: 110,
  rangeHours: 24,
})

const gradientId = `usage-gradient-${Math.random().toString(36).slice(2)}`
const hoveredPoint = ref<{
  x: number
  y: number
  value: number
  timeLabel: string
  tooltipLeft: number
} | null>(null)

// Maps a percentage (0–100) to SVG Y coordinate
function percentToSvgY(pct: number): number {
  return SVG_H - pct
}

const sanitizedValues = computed(() =>
  props.values.map((v) => Math.max(0, Math.min(100, Number(v) || 0)))
)

const points = computed(() => {
  const values = sanitizedValues.value
  if (!values.length) return []

  const stepX = values.length > 1 ? SVG_W / (values.length - 1) : SVG_W

  return values.map((value, index) => ({
    x: Math.round(stepX * index * 100) / 100,
    y: Math.round(percentToSvgY(value) * 100) / 100,
  }))
})

const linePath = computed(() => {
  if (points.value.length < 2) return ''
  return points.value.map((p) => `${p.x},${p.y}`).join(' ')
})

const areaPath = computed(() => {
  if (points.value.length < 2) return ''
  const first = points.value[0]
  const last = points.value[points.value.length - 1]
  if (!first || !last) return ''
  const poly = points.value.map((p) => `${p.x},${p.y}`).join(' ')
  return `M ${first.x},${SVG_H} L ${poly} L ${last.x},${SVG_H} Z`
})

const xTicks = computed(() => {
  const ts = props.timestamps
  if (!ts || ts.length < 2) return []

  const total = ts.length
  const step = (total - 1) / (X_TICK_COUNT - 1)

  return Array.from({ length: X_TICK_COUNT }, (_, i) => {
    const index = Math.round(Math.min(i * step, total - 1))
    const position = i === 0 ? 'x-tick--start' : i === X_TICK_COUNT - 1 ? 'x-tick--end' : 'x-tick--center'
    return {
      index,
      percent: (index / (total - 1)) * 100,
      label: formatXLabel(ts[index]!, props.rangeHours ?? 24),
      position,
    }
  })
})

function formatXLabel(iso: string, rangeHours: number): string {
  const d = new Date(iso)
  if (rangeHours <= 24) {
    return d.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' })
  }
  return d.toLocaleDateString('pt-BR', { day: '2-digit', month: '2-digit' })
}

function formatTooltipLabel(iso: string, rangeHours: number): string {
  const d = new Date(iso)
  if (rangeHours <= 24) {
    return d.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' })
  }
  return d.toLocaleString('pt-BR', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })
}

function handlePointerMove(event: MouseEvent): void {
  if (!points.value.length) {
    hoveredPoint.value = null
    return
  }

  const svg = event.currentTarget as SVGElement | null
  if (!svg) {
    hoveredPoint.value = null
    return
  }

  const svgRect = svg.getBoundingClientRect()
  if (svgRect.width <= 0) {
    hoveredPoint.value = null
    return
  }

  const pointerX = event.clientX - svgRect.left
  const ratioX = Math.max(0, Math.min(1, pointerX / svgRect.width))
  const index = Math.round(ratioX * (points.value.length - 1))
  const point = points.value[index]
  const value = sanitizedValues.value[index]

  if (!point || value === undefined) {
    hoveredPoint.value = null
    return
  }

  const ts = props.timestamps

  hoveredPoint.value = {
    x: point.x,
    y: point.y,
    value,
    timeLabel: ts?.[index] ? formatTooltipLabel(ts[index]!, props.rangeHours ?? 24) : '',
    tooltipLeft: pointerX,
  }
}

function clearHoveredPoint(): void {
  hoveredPoint.value = null
}
</script>

<style scoped>
.chart-root {
  display: flex;
  align-items: stretch;
  width: 100%;
  gap: 4px;
}

/* Y-axis: stacked labels from 100% (top) to 0% (bottom), aligned to SVG height only */
.chart-y-axis {
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  flex-shrink: 0;
  align-self: flex-start;
  width: 36px;
  padding: 0;
}

.y-tick-label {
  font-size: 10px;
  line-height: 1;
  color: rgba(var(--v-theme-on-surface), 0.45);
  text-align: right;
  user-select: none;
}

/* Chart column: SVG on top, X-axis below */
.chart-column {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.chart-area-wrapper {
  position: relative;
  width: 100%;
  flex-shrink: 0;
}

.chart-svg {
  width: 100%;
  height: 100%;
  display: block;
}

.grid-line {
  stroke: rgba(var(--v-border-color), 0.16);
  stroke-width: 1;
}

.line-path {
  fill: none;
  stroke-width: 2.5;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.hover-dot {
  stroke: rgb(var(--v-theme-surface));
  stroke-width: 2;
}

.chart-tooltip {
  position: absolute;
  top: 4px;
  transform: translate(-50%, 0);
  padding: 4px 8px;
  border-radius: 8px;
  background: rgba(20, 20, 20, 0.92);
  color: #fff;
  font-size: 11px;
  font-weight: 600;
  pointer-events: none;
  white-space: nowrap;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1px;
}

.tooltip-time {
  font-size: 10px;
  font-weight: 400;
  opacity: 0.8;
}

/* X-axis: labels positioned via left % */
.chart-x-axis {
  position: relative;
  height: 20px;
  margin-top: 2px;
}

.x-tick-label {
  position: absolute;
  font-size: 10px;
  line-height: 1;
  color: rgba(var(--v-theme-on-surface), 0.45);
  white-space: nowrap;
  user-select: none;
}

.x-tick--center {
  transform: translateX(-50%);
}

.x-tick--start {
  transform: none;
}

.x-tick--end {
  transform: translateX(-100%);
}
</style>
