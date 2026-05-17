<script setup>
import { computed, ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  PhysicalPosition,
  currentMonitor,
  getCurrentWindow
} from '@tauri-apps/api/window'

const refreshInterval = 1000
const dockRevealRatio = 0.1
const visibleStrip = 10
const dockEvaluationDelay = 90
const collapseDelay = 160
const topContactMargin = 2

let intervalId = null
let leaveTimer = null
let moveTimer = null
let unlistenMoved = null
let unlistenOpacity = null
let dockState = null
const appWindow = getCurrentWindow()
const viewMode = new URLSearchParams(window.location.search).get('mode') || 'monitor'
const isMonitorView = viewMode === 'monitor'
const isSettingsView = viewMode === 'settings'
document.documentElement.dataset.viewMode = viewMode
document.body.dataset.viewMode = viewMode

const cpuUsage = ref(0)
const gpuUsage = ref(0)
const memoryUsage = ref(0)
const topCpuProcesses = ref([])
const topGpuProcesses = ref([])
const topMemoryProcesses = ref([])
const selectedMetric = ref(null)
const opacityPercent = ref(92)
const appliedOpacityPercent = ref(92)

const displayedProcesses = computed(() => {
  if (selectedMetric.value === 'gpu') {
    return topGpuProcesses.value
  }

  if (selectedMetric.value === 'memory') {
    return topMemoryProcesses.value
  }

  return topCpuProcesses.value
})

async function fetchData() {
  try {
    const result = await invoke('get_system_info')
    cpuUsage.value = result.cpu_percent
    gpuUsage.value = result.gpu_percent
    memoryUsage.value = result.memory_percent
    topCpuProcesses.value = result.top_processes.slice(0, 5)
    topGpuProcesses.value = result.top_gpu_processes.slice(0, 5)
    topMemoryProcesses.value = result.top_memory_processes.slice(0, 5)
  } catch (error) {
    console.error('Failed to fetch system info:', error)
  }
}

function formatPercent(value) {
  return Math.round(value)
}

function toggleMetric(metric) {
  selectedMetric.value = selectedMetric.value === metric ? null : metric
}

async function startDrag(event) {
  if (event.button !== 0 || event.target.closest('.interactive')) {
    return
  }

  clearLeaveTimer()
  dockState = null

  try {
    await appWindow.startDragging()
  } catch (error) {
    console.error('Failed to start dragging:', error)
  }
}

async function closeWindow() {
  try {
    await invoke('hide_main_window')
  } catch (error) {
    console.error('Failed to hide window:', error)
  }
}

async function applyOpacity() {
  try {
    const nextOpacity = await invoke('apply_window_opacity', {
      opacityPercent: opacityPercent.value
    })
    setPanelOpacity(nextOpacity)
    appliedOpacityPercent.value = nextOpacity
    opacityPercent.value = nextOpacity
  } catch (error) {
    console.error('Failed to apply opacity:', error)
  }
}

async function loadOpacity() {
  try {
    const opacity = await invoke('get_window_opacity')
    opacityPercent.value = opacity
    appliedOpacityPercent.value = opacity
    setPanelOpacity(opacity)
  } catch (error) {
    console.error('Failed to load opacity:', error)
  }
}

function setPanelOpacity(percent) {
  const opacity = clamp(Number(percent) / 100, 0.4, 1)
  document.documentElement.style.setProperty('--panel-opacity', opacity.toFixed(2))
}

function updateOpacityFromInput(event) {
  opacityPercent.value = clamp(Number(event.target.value) || 40, 40, 100)
}

async function closeSettingsWindow() {
  try {
    await appWindow.hide()
  } catch (error) {
    console.error('Failed to close settings window:', error)
  }
}

async function evaluateDocking() {
  try {
    const edge = await getNearestEdge()
    if (edge) {
      await collapseToEdge(edge)
    }
  } catch (error) {
    console.error('Failed to evaluate docking:', error)
  }
}

async function getNearestEdge() {
  const monitor = await currentMonitor()
  if (!monitor) {
    return null
  }

  const position = await appWindow.outerPosition()
  const size = await appWindow.outerSize()
  const bounds = monitorBounds(monitor)
  const horizontalThreshold = Math.max(1, size.width * dockRevealRatio)
  const verticalThreshold = Math.max(1, size.height * dockRevealRatio)

  const candidates = []

  if (position.x <= bounds.left - horizontalThreshold) {
    candidates.push({ edge: 'left', value: Math.abs(position.x - bounds.left) })
  }
  if (position.x + size.width >= bounds.right + horizontalThreshold) {
    candidates.push({ edge: 'right', value: Math.abs(bounds.right - (position.x + size.width)) })
  }
  if (position.y <= bounds.top + topContactMargin) {
    candidates.push({ edge: 'top', value: Math.abs(position.y - (bounds.top - verticalThreshold)) })
  }
  if (position.y + size.height >= bounds.bottom + verticalThreshold) {
    candidates.push({ edge: 'bottom', value: Math.abs(bounds.bottom - (position.y + size.height)) })
  }

  candidates.sort((a, b) => a.value - b.value)
  return candidates[0]?.edge ?? null
}

async function collapseToEdge(edge) {
  const monitor = await currentMonitor()
  if (!monitor) {
    return
  }

  const position = await appWindow.outerPosition()
  const size = await appWindow.outerSize()
  const bounds = monitorBounds(monitor)

  dockState = {
    edge,
    expanded: {
      x: clamp(position.x, bounds.left, bounds.right - size.width),
      y: clamp(position.y, bounds.top, bounds.bottom - size.height)
    }
  }

  await appWindow.setPosition(new PhysicalPosition(
    dockedX(edge, bounds, size, dockState.expanded.x),
    dockedY(edge, bounds, size, dockState.expanded.y)
  ))
}

async function expandFromEdge() {
  if (!dockState) {
    return
  }

  clearLeaveTimer()

  try {
    const monitor = await currentMonitor()
    const size = await appWindow.outerSize()
    if (!monitor) {
      return
    }

    const bounds = monitorBounds(monitor)
    const nextPosition = expandedPosition(dockState.edge, bounds, size, dockState.expanded)
    await appWindow.setPosition(new PhysicalPosition(nextPosition.x, nextPosition.y))
    dockState.expanded = nextPosition
  } catch (error) {
    console.error('Failed to expand docked window:', error)
  }
}

function scheduleCollapse() {
  if (!dockState) {
    return
  }

  clearLeaveTimer()
  leaveTimer = window.setTimeout(async () => {
    try {
      if (dockState) {
        await collapseToEdge(dockState.edge)
      }
    } catch (error) {
      console.error('Failed to collapse docked window:', error)
    }
  }, collapseDelay)
}

function clearLeaveTimer() {
  if (leaveTimer) {
    window.clearTimeout(leaveTimer)
    leaveTimer = null
  }
}

function scheduleDockEvaluation() {
  if (dockState) {
    return
  }

  clearMoveTimer()
  moveTimer = window.setTimeout(evaluateDocking, dockEvaluationDelay)
}

function clearMoveTimer() {
  if (moveTimer) {
    window.clearTimeout(moveTimer)
    moveTimer = null
  }
}

function monitorBounds(monitor) {
  return {
    left: monitor.position.x,
    top: monitor.position.y,
    right: monitor.position.x + monitor.size.width,
    bottom: monitor.position.y + monitor.size.height
  }
}

function expandedPosition(edge, bounds, size, previous) {
  if (edge === 'left') {
    return { x: bounds.left, y: clamp(previous.y, bounds.top, bounds.bottom - size.height) }
  }

  if (edge === 'right') {
    return { x: bounds.right - size.width, y: clamp(previous.y, bounds.top, bounds.bottom - size.height) }
  }

  if (edge === 'top') {
    return { x: clamp(previous.x, bounds.left, bounds.right - size.width), y: bounds.top }
  }

  return {
    x: clamp(previous.x, bounds.left, bounds.right - size.width),
    y: bounds.bottom - size.height
  }
}

function dockedX(edge, bounds, size, fallback) {
  if (edge === 'left') {
    return bounds.left - size.width + visibleStrip
  }

  if (edge === 'right') {
    return bounds.right - visibleStrip
  }

  return clamp(fallback, bounds.left, bounds.right - size.width)
}

function dockedY(edge, bounds, size, fallback) {
  if (edge === 'top') {
    return bounds.top - size.height + visibleStrip
  }

  if (edge === 'bottom') {
    return bounds.bottom - visibleStrip
  }

  return clamp(fallback, bounds.top, bounds.bottom - size.height)
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max)
}

onMounted(async () => {
  if (isMonitorView) {
    await loadOpacity()
    unlistenOpacity = await listen('opacity-changed', (event) => {
      const opacity = Number(event.payload)
      opacityPercent.value = opacity
      appliedOpacityPercent.value = opacity
      setPanelOpacity(opacity)
    })
    fetchData()
    intervalId = setInterval(fetchData, refreshInterval)
    unlistenMoved = await appWindow.onMoved(scheduleDockEvaluation)
  }

  if (isSettingsView) {
    await loadOpacity()
  }
})

onUnmounted(() => {
  if (intervalId) {
    clearInterval(intervalId)
  }
  clearLeaveTimer()
  clearMoveTimer()
  if (unlistenMoved) {
    unlistenMoved()
  }
  if (unlistenOpacity) {
    unlistenOpacity()
  }
})
</script>

<template>
  <main
    v-if="isMonitorView"
    class="float-panel"
    @pointerdown="startDrag"
    @pointerenter="expandFromEdge"
    @pointerleave="scheduleCollapse"
  >
    <button class="close-button interactive" type="button" aria-label="Close" @pointerdown.stop @click.stop="closeWindow">
      x
    </button>
    <div class="usage-line">
      <button
        class="usage-button interactive"
        :class="{ selected: selectedMetric === 'cpu' }"
        type="button"
        @pointerdown.stop
        @click.stop="toggleMetric('cpu')"
      >
        CPU:{{ formatPercent(cpuUsage) }}%
      </button>
      <button
        class="usage-button interactive"
        :class="{ selected: selectedMetric === 'gpu' }"
        type="button"
        @pointerdown.stop
        @click.stop="toggleMetric('gpu')"
      >
        GPU:{{ formatPercent(gpuUsage) }}%
      </button>
      <button
        class="usage-button interactive"
        :class="{ selected: selectedMetric === 'memory' }"
        type="button"
        @pointerdown.stop
        @click.stop="toggleMetric('memory')"
      >
        Memory:{{ formatPercent(memoryUsage) }}%
      </button>
    </div>
    <div class="process-list">
      <div
        v-for="proc in displayedProcesses"
        :key="`${selectedMetric || 'default'}-${proc.pid}`"
        class="process-row"
      >
        <span class="process-name">{{ proc.name }}</span>
        <span class="process-usage">CPU:{{ formatPercent(proc.cpu_percent) }}%</span>
        <span class="process-usage">GPU:{{ formatPercent(proc.gpu_percent) }}%</span>
        <span class="process-usage">MEM:{{ formatPercent(proc.memory_percent) }}%</span>
      </div>
    </div>
  </main>

  <main
    v-else-if="isSettingsView"
    class="settings-panel"
    @pointerdown="startDrag"
  >
    <header class="settings-header">
      <div>
        <h1>设置</h1>
      </div>
      <button class="close-button settings-close interactive" type="button" aria-label="Close" @pointerdown.stop @click.stop="closeSettingsWindow">
        x
      </button>
    </header>

    <div class="settings-content">
      <p class="settings-description">调整监控窗口不透明度</p>

      <section class="opacity-control">
        <div class="opacity-label-row">
          <span>不透明度</span>
          <strong>{{ opacityPercent }}%</strong>
        </div>
        <input
          class="opacity-slider interactive"
          type="range"
          min="40"
          max="100"
          step="1"
          :value="opacityPercent"
          @input="updateOpacityFromInput"
        >
        <div class="opacity-input-row">
          <input
            class="opacity-number interactive"
            type="number"
            min="40"
            max="100"
            step="1"
            :value="opacityPercent"
            @input="updateOpacityFromInput"
          >
          <span>%</span>
        </div>
      </section>

      <footer class="settings-actions">
        <span class="settings-status">当前 {{ appliedOpacityPercent }}%</span>
        <button class="apply-button interactive" type="button" @pointerdown.stop @pointerup.stop.prevent="applyOpacity">
          应用
        </button>
      </footer>
    </div>
  </main>
</template>
