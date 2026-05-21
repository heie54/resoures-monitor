<script setup>
import { computed, ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  PhysicalPosition,
  currentMonitor,
  getCurrentWindow
} from '@tauri-apps/api/window'
import {
  applyAppearanceConfig,
  cloneAppearanceConfig,
  configsEqual,
  defaultAppearanceConfig,
  normalizeAppearanceConfig
} from './appearance'
import {
  closeProcessMenuState,
  createClosedProcessMenuState
} from './processMenuState'
import { formatProcessPercent, formatTotalPercent } from './percentFormat'

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
let unlistenAppearance = null
let themeMediaQuery = null
let themeChangeHandler = null
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
const frozenProcesses = ref(null)
const processMenu = ref(createClosedProcessMenuState())
const appearanceDraft = ref(cloneAppearanceConfig(defaultAppearanceConfig))
const savedAppearance = ref(cloneAppearanceConfig(defaultAppearanceConfig))
const appearanceStatus = ref('')

const displayedProcesses = computed(() => {
  if (frozenProcesses.value) {
    return frozenProcesses.value
  }

  if (selectedMetric.value === 'gpu') {
    return topGpuProcesses.value
  }

  if (selectedMetric.value === 'memory') {
    return topMemoryProcesses.value
  }

  return topCpuProcesses.value
})

const appearanceChanged = computed(() => !configsEqual(appearanceDraft.value, savedAppearance.value))
const contextProcessPid = computed(() => processMenu.value.process?.pid ?? null)

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

function toggleMetric(metric) {
  closeProcessMenu()
  selectedMetric.value = selectedMetric.value === metric ? null : metric
}

function openProcessMenu(event, proc) {
  event.preventDefault()
  event.stopPropagation()
  clearLeaveTimer()

  const snapshot = displayedProcesses.value.map((process) => ({ ...process }))
  const selectedProcess = snapshot.find((process) => process.pid === proc.pid) ?? { ...proc }
  const menuWidth = 154
  const menuHeight = 76

  frozenProcesses.value = snapshot
  processMenu.value = {
    visible: true,
    x: clamp(event.clientX, 8, Math.max(8, window.innerWidth - menuWidth - 8)),
    y: clamp(event.clientY, 8, Math.max(8, window.innerHeight - menuHeight - 8)),
    process: selectedProcess,
    busy: false,
    message: ''
  }
}

function closeProcessMenu(options = {}) {
  const nextMenu = closeProcessMenuState(processMenu.value, options)
  if (nextMenu === processMenu.value) {
    return
  }

  processMenu.value = nextMenu
  frozenProcesses.value = null
}

function handleEscape(event) {
  if (event.key === 'Escape') {
    closeProcessMenu()
  }
}

async function terminateSelectedProcess() {
  const proc = processMenu.value.process
  if (!proc || processMenu.value.busy) {
    return
  }

  const confirmed = window.confirm(`结束进程 "${proc.name}" (PID ${proc.pid})？`)
  if (!confirmed) {
    return
  }

  processMenu.value = { ...processMenu.value, busy: true, message: '正在结束进程...' }

  try {
    await invoke('terminate_process', { pid: proc.pid })
    processMenu.value = { ...processMenu.value, busy: false, message: '已请求结束进程' }
    window.setTimeout(() => {
      closeProcessMenu()
      fetchData()
    }, 300)
  } catch (error) {
    console.error('Failed to terminate process:', error)
    processMenu.value = {
      ...processMenu.value,
      busy: false,
      message: String(error)
    }
  }
}

async function openSelectedProcessLocation() {
  const proc = processMenu.value.process
  if (!proc || processMenu.value.busy) {
    return
  }

  processMenu.value = { ...processMenu.value, busy: true, message: '正在打开位置...' }

  try {
    await invoke('open_process_location', { pid: proc.pid })
    closeProcessMenu({ force: true })
  } catch (error) {
    console.error('Failed to open process location:', error)
    processMenu.value = {
      ...processMenu.value,
      busy: false,
      message: String(error)
    }
  }
}

async function startDrag(event) {
  if (event.button === 0 && processMenu.value.visible) {
    closeProcessMenu()
    return
  }

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

async function startResize(event, direction) {
  if (event.button !== 0) {
    return
  }

  try {
    await appWindow.startResizeDragging(direction)
  } catch (error) {
    console.error('Failed to start resizing:', error)
  }
}

async function closeWindow() {
  try {
    await invoke('hide_main_window')
  } catch (error) {
    console.error('Failed to hide window:', error)
  }
}

async function loadAppearance() {
  try {
    const config = normalizeAppearanceConfig(await invoke('get_appearance_config'))
    savedAppearance.value = cloneAppearanceConfig(config)
    appearanceDraft.value = cloneAppearanceConfig(config)
    applyAppearanceConfig(config)
  } catch (error) {
    console.error('Failed to load appearance config:', error)
    applyAppearanceConfig(defaultAppearanceConfig)
  }
}

function updateAppearanceValue(key, value) {
  appearanceStatus.value = ''
  const next = normalizeAppearanceConfig({
    ...appearanceDraft.value,
    [key]: value
  })

  appearanceDraft.value = next
  applyAppearanceConfig(next)
}

async function saveAppearance() {
  try {
    const config = normalizeAppearanceConfig(
      await invoke('save_appearance_config', {
        config: appearanceDraft.value
      })
    )
    savedAppearance.value = cloneAppearanceConfig(config)
    appearanceDraft.value = cloneAppearanceConfig(config)
    applyAppearanceConfig(config)
    appearanceStatus.value = '已保存'
  } catch (error) {
    console.error('Failed to save appearance config:', error)
    appearanceStatus.value = '保存失败'
  }
}

async function resetAppearance() {
  try {
    const config = normalizeAppearanceConfig(await invoke('reset_appearance_config'))
    savedAppearance.value = cloneAppearanceConfig(config)
    appearanceDraft.value = cloneAppearanceConfig(config)
    applyAppearanceConfig(config)
    appearanceStatus.value = '已恢复默认'
  } catch (error) {
    console.error('Failed to reset appearance config:', error)
    appearanceStatus.value = '恢复失败'
  }
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
  await loadAppearance()
  themeMediaQuery = window.matchMedia?.('(prefers-color-scheme: light)')
  themeChangeHandler = () => {
    if (appearanceDraft.value.themeMode === 'system') {
      applyAppearanceConfig(appearanceDraft.value)
    }
  }
  themeMediaQuery?.addEventListener?.('change', themeChangeHandler)
  unlistenAppearance = await listen('appearance-changed', (event) => {
    const config = normalizeAppearanceConfig(event.payload)
    savedAppearance.value = cloneAppearanceConfig(config)
    appearanceDraft.value = cloneAppearanceConfig(config)
    applyAppearanceConfig(config)
  })

  if (isMonitorView) {
    fetchData()
    intervalId = setInterval(fetchData, refreshInterval)
    unlistenMoved = await appWindow.onMoved(scheduleDockEvaluation)
    window.addEventListener('keydown', handleEscape)
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
  if (unlistenAppearance) {
    unlistenAppearance()
  }
  window.removeEventListener('keydown', handleEscape)
  themeMediaQuery?.removeEventListener?.('change', themeChangeHandler)
  themeMediaQuery = null
  themeChangeHandler = null
})
</script>

<template>
  <main
    v-if="isMonitorView"
    class="float-panel"
    @pointerdown="startDrag"
    @pointerenter="expandFromEdge"
    @pointerleave="scheduleCollapse"
    @click="closeProcessMenu"
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
        CPU:{{ formatTotalPercent(cpuUsage) }}%
      </button>
      <button
        class="usage-button interactive"
        :class="{ selected: selectedMetric === 'gpu' }"
        type="button"
        @pointerdown.stop
        @click.stop="toggleMetric('gpu')"
      >
        GPU:{{ formatTotalPercent(gpuUsage) }}%
      </button>
      <button
        class="usage-button interactive"
        :class="{ selected: selectedMetric === 'memory' }"
        type="button"
        @pointerdown.stop
        @click.stop="toggleMetric('memory')"
      >
        Memory:{{ formatTotalPercent(memoryUsage) }}%
      </button>
    </div>
    <div class="process-list interactive">
      <div
        v-for="proc in displayedProcesses"
        :key="`${selectedMetric || 'default'}-${proc.pid}`"
        class="process-row"
        :class="{ focused: contextProcessPid === proc.pid }"
        @contextmenu="openProcessMenu($event, proc)"
      >
        <span class="process-name">{{ proc.name }}</span>
        <span class="process-usage">CPU:{{ formatProcessPercent(proc.cpu_percent) }}%</span>
        <span class="process-usage">GPU:{{ formatProcessPercent(proc.gpu_percent) }}%</span>
        <span class="process-usage">MEM:{{ formatProcessPercent(proc.memory_percent) }}%</span>
      </div>
    </div>
    <div
      v-if="processMenu.visible"
      class="process-context-menu interactive"
      :style="{ left: `${processMenu.x}px`, top: `${processMenu.y}px` }"
      @pointerdown.stop
      @contextmenu.prevent.stop
    >
      <button type="button" class="context-menu-item danger" :disabled="processMenu.busy" @click="terminateSelectedProcess">
        结束进程
      </button>
      <button type="button" class="context-menu-item" :disabled="processMenu.busy" @click="openSelectedProcessLocation">
        打开文件所在位置
      </button>
      <p v-if="processMenu.message" class="context-menu-message">{{ processMenu.message }}</p>
    </div>
  </main>

  <main
    v-else-if="isSettingsView"
    class="settings-panel"
  >
    <div class="resize-edge resize-edge-top" @mousedown.stop.prevent="startResize($event, 'North')"></div>
    <div class="resize-edge resize-edge-right" @mousedown.stop.prevent="startResize($event, 'East')"></div>
    <div class="resize-edge resize-edge-bottom" @mousedown.stop.prevent="startResize($event, 'South')"></div>
    <div class="resize-edge resize-edge-left" @mousedown.stop.prevent="startResize($event, 'West')"></div>
    <div class="resize-corner resize-corner-top-left" @mousedown.stop.prevent="startResize($event, 'NorthWest')"></div>
    <div class="resize-corner resize-corner-top-right" @mousedown.stop.prevent="startResize($event, 'NorthEast')"></div>
    <div class="resize-corner resize-corner-bottom-right" @mousedown.stop.prevent="startResize($event, 'SouthEast')"></div>
    <div class="resize-corner resize-corner-bottom-left" @mousedown.stop.prevent="startResize($event, 'SouthWest')"></div>

    <header class="settings-header" @pointerdown="startDrag">
      <div>
        <h1>设置</h1>
      </div>
      <button class="close-button settings-close interactive" type="button" aria-label="Close" @pointerdown.stop @click.stop="closeSettingsWindow">
        x
      </button>
    </header>

    <div class="settings-content">
      <p class="settings-description">调整主窗口和设置窗口的外观，所有修改都会实时预览。</p>

      <section class="settings-section">
        <h2>样式设置</h2>

        <div class="setting-row vertical">
          <div class="setting-label-row">
            <span>窗口不透明度</span>
            <strong>{{ appearanceDraft.opacity }}%</strong>
          </div>
          <input
            class="setting-slider interactive"
            type="range"
            min="40"
            max="100"
            step="1"
            :value="appearanceDraft.opacity"
            @input="updateAppearanceValue('opacity', $event.target.value)"
          >
        </div>

        <div class="setting-row vertical">
          <div class="setting-label-row">
            <span>圆角大小</span>
            <strong>{{ appearanceDraft.borderRadius }}px</strong>
          </div>
          <input
            class="setting-slider interactive"
            type="range"
            min="0"
            max="32"
            step="1"
            :value="appearanceDraft.borderRadius"
            @input="updateAppearanceValue('borderRadius', $event.target.value)"
          >
        </div>

        <div class="setting-row">
          <span>主题模式</span>
          <div class="segmented-control">
            <button class="segmented-button interactive" :class="{ selected: appearanceDraft.themeMode === 'system' }" type="button" @pointerdown.stop @click.stop="updateAppearanceValue('themeMode', 'system')">跟随系统</button>
            <button class="segmented-button interactive" :class="{ selected: appearanceDraft.themeMode === 'light' }" type="button" @pointerdown.stop @click.stop="updateAppearanceValue('themeMode', 'light')">浅色</button>
            <button class="segmented-button interactive" :class="{ selected: appearanceDraft.themeMode === 'dark' }" type="button" @pointerdown.stop @click.stop="updateAppearanceValue('themeMode', 'dark')">深色</button>
          </div>
        </div>

        <div class="setting-row">
          <span>主题色</span>
          <input class="color-input interactive" type="color" :value="appearanceDraft.accentColor" @input="updateAppearanceValue('accentColor', $event.target.value)">
        </div>

        <div class="setting-row">
          <span>字体大小</span>
          <div class="segmented-control compact">
            <button class="segmented-button interactive" :class="{ selected: appearanceDraft.fontSize === 'small' }" type="button" @pointerdown.stop @click.stop="updateAppearanceValue('fontSize', 'small')">小</button>
            <button class="segmented-button interactive" :class="{ selected: appearanceDraft.fontSize === 'medium' }" type="button" @pointerdown.stop @click.stop="updateAppearanceValue('fontSize', 'medium')">中</button>
            <button class="segmented-button interactive" :class="{ selected: appearanceDraft.fontSize === 'large' }" type="button" @pointerdown.stop @click.stop="updateAppearanceValue('fontSize', 'large')">大</button>
          </div>
        </div>

        <label class="setting-toggle">
          <span>背景模糊</span>
          <input class="interactive" type="checkbox" :checked="appearanceDraft.backgroundBlur" @change="updateAppearanceValue('backgroundBlur', $event.target.checked)">
        </label>

        <label class="setting-toggle">
          <span>动画效果</span>
          <input class="interactive" type="checkbox" :checked="appearanceDraft.animations" @change="updateAppearanceValue('animations', $event.target.checked)">
        </label>

        <label class="setting-toggle">
          <span>窗口阴影</span>
          <input class="interactive" type="checkbox" :checked="appearanceDraft.windowShadow" @change="updateAppearanceValue('windowShadow', $event.target.checked)">
        </label>
      </section>

      <footer class="settings-actions">
        <span class="settings-status">{{ appearanceStatus || (appearanceChanged ? '未保存' : '已同步') }}</span>
        <div class="settings-action-buttons">
          <button class="secondary-button interactive" type="button" @pointerdown.stop @click.stop="resetAppearance">恢复默认设置</button>
          <button class="apply-button interactive" type="button" :disabled="!appearanceChanged" @pointerdown.stop @click.stop="saveAppearance">保存</button>
        </div>
      </footer>
    </div>
  </main>
</template>
