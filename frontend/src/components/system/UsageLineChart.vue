<template>
  <div class="chart-root">
    <svg
      :viewBox="`0 0 ${viewBoxWidth} ${viewBoxHeight}`"
      class="chart-svg"
      preserveAspectRatio="none"
      role="img"
      aria-label="Histórico de uso percentual"
      @mouseleave="clearHoveredPoint"
      @mousemove="handlePointerMove"
    >
      <defs>
        <linearGradient :id="gradientId" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" :stop-color="rgbaColor" stop-opacity="0.35" />
          <stop offset="100%" :stop-color="rgbaColor" stop-opacity="0.02" />
        </linearGradient>
      </defs>

      <line
        v-for="line in gridLines"
        :key="line.y"
        :x1="0"
        :x2="viewBoxWidth"
        :y1="line.y"
        :y2="line.y"
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
        :stroke="rgbaColor"
        class="line-path"
      />

      <circle
        v-if="hoveredPoint"
        :cx="hoveredPoint.x"
        :cy="hoveredPoint.y"
        r="4"
        :fill="rgbaColor"
        class="hover-dot"
      />
    </svg>

    <div
      v-if="hoveredPoint"
      class="chart-tooltip"
      :style="{ left: `${hoveredPoint.tooltipLeft}px` }"
    >
      {{ hoveredPoint.value.toFixed(1) }}%
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'

const props = withDefaults(defineProps<{
  values: number[]
  color?: string
  height?: number
}>(), {
  color: 'rgb(var(--v-theme-primary))',
  height: 110,
})

const viewBoxWidth = 360
const viewBoxHeight = computed(() => Math.max(60, props.height))
const gradientId = `usage-gradient-${Math.random().toString(36).slice(2)}`
const hoveredPoint = ref<{
  x: number
  y: number
  value: number
  tooltipLeft: number
} | null>(null)

const sanitizedValues = computed(() =>
  props.values.map((value) => Math.max(0, Math.min(100, Number(value) || 0)))
)

const gridLines = computed(() => {
  return [0.25, 0.5, 0.75].map((ratio) => ({
    y: Math.round(viewBoxHeight.value * ratio),
  }))
})

const points = computed(() => {
  const values = sanitizedValues.value
  if (!values.length) return []

  const stepX = values.length > 1 ? viewBoxWidth / (values.length - 1) : viewBoxWidth

  return values.map((value, index) => {
    const x = Math.round(stepX * index * 100) / 100
    const y = Math.round(((100 - value) / 100) * viewBoxHeight.value * 100) / 100
    return { x, y }
  })
})

const linePath = computed(() => {
  if (points.value.length < 2) {
    return ''
  }

  return points.value.map((point) => `${point.x},${point.y}`).join(' ')
})

const areaPath = computed(() => {
  if (points.value.length < 2) {
    return ''
  }

  const first = points.value[0]
  const last = points.value[points.value.length - 1]
  if (!first || !last) {
    return ''
  }
  const polylinePart = points.value.map((point) => `${point.x},${point.y}`).join(' ')

  return `M ${first.x},${viewBoxHeight.value} L ${polylinePart} L ${last.x},${viewBoxHeight.value} Z`
})

const rgbaColor = computed(() => props.color)

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

  hoveredPoint.value = {
    x: point.x,
    y: point.y,
    value,
    tooltipLeft: ratioX * 100,
  }
}

function clearHoveredPoint(): void {
  hoveredPoint.value = null
}
</script>

<style scoped>
.chart-root {
  position: relative;
  width: 100%;
}

.chart-svg {
  width: 100%;
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
  top: -2px;
  transform: translate(-50%, -100%);
  padding: 4px 8px;
  border-radius: 8px;
  background: rgba(20, 20, 20, 0.92);
  color: #fff;
  font-size: 12px;
  font-weight: 600;
  pointer-events: none;
  white-space: nowrap;
}
</style>
