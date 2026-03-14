<template>
  <div
    class="app"
    :class="{
      dragging: isDragging,
      passthrough: passThroughEnabled,
      'debug-border': showDebugBorder
    }"
  >
    <div class="dog-zone">
      <div
        class="dog-container"
        :class="{
          dragging: isDragging,
          sleeping: petVisualState === 'sleep',
          typing: petVisualState === 'type1' || petVisualState === 'type2',
          burst: petVisualState === 'burst'
        }"
        @mousedown.left="onDogMouseDown"
        :title="passThroughEnabled ? '当前是穿透模式，按 Ctrl+Shift+D 恢复交互' : '按住可以拖动'"
      >
        <img
          :src="currentFrame"
          class="dog-sprite"
          draggable="false"
          alt="pet"
        />

        <div v-if="petVisualState === 'sleep'" class="zzz">z z z</div>
        <div v-if="petVisualState === 'burst'" class="burst-effect">💥</div>
      </div>
    </div>

    <div class="keyboard-dock" v-if="!showPanel && !passThroughEnabled">
      <div class="vk-row">
        <div
          v-for="key in KEYBOARD_ROWS[0]"
          :key="key"
          class="vk-key"
          :class="keyClass(key)"
        >
          {{ keyLabel(key) }}
        </div>
      </div>
      <div class="vk-row">
        <div
          v-for="key in KEYBOARD_ROWS[1]"
          :key="key"
          class="vk-key"
          :class="keyClass(key)"
        >
          {{ keyLabel(key) }}
        </div>
      </div>
      <div class="vk-row">
        <div
          v-for="key in KEYBOARD_ROWS[2]"
          :key="key"
          class="vk-key"
          :class="keyClass(key)"
        >
          {{ keyLabel(key) }}
        </div>
      </div>
      <div class="vk-row">
        <div
          v-for="key in KEYBOARD_ROWS[3]"
          :key="key"
          class="vk-key"
          :class="keyClass(key)"
        >
          {{ keyLabel(key) }}
        </div>
      </div>
    </div>

    <div v-if="!passThroughEnabled" class="debug-actions">
      <button @click="openPanel">统计</button>
      <button @click="getAIAnalysis">AI</button>
      <button @click="togglePassThrough">穿透</button>
    </div>

    <Transition name="slide-up">
      <div
        v-if="aiAnalysis && !showPanel && !passThroughEnabled"
        class="ai-bubble"
      >
        <div class="ai-text">{{ aiAnalysis }}</div>
        <button class="close-btn" @click="closeAI">×</button>
      </div>
    </Transition>

    <Transition name="fade">
      <div v-if="showPanel" class="stats-panel" @mousedown.stop>
        <button class="close-btn panel-close" @click="closePanel">×</button>

        <div class="panel-header">
          <h3>📊 本周战绩</h3>
          <button class="panel-mode-btn" @click="enablePassThrough">
            开启穿透
          </button>
        </div>

        <div class="panel-subtitle">
          当前状态：
          <span class="state-tag">{{ visualStateLabel }}</span>
          <span class="state-meta">WPM {{ currentWPM }}</span>
          <span class="state-meta">总击键 {{ todayCount }}</span>
          <span class="state-meta">最近键 {{ keyLabel(lastKey || 'Unknown') }}</span>
        </div>

        <div class="week-chart">
          <div
            v-for="day in weekStats"
            :key="day.date"
            class="bar-col"
          >
            <div class="bar-wrap">
              <div
                class="bar"
                :style="{ height: barHeight(day.keys) + 'px' }"
                :class="{ today: isToday(day.date) }"
              ></div>
            </div>
            <div class="bar-label">{{ shortDate(day.date) }}</div>
          </div>
        </div>

        <div class="week-total">
          总计 {{ weekTotal.toLocaleString() }} 次击键
        </div>

        <div class="api-section">
          <input
            v-model="apiKey"
            type="password"
            placeholder="Claude API Key（可选）"
            class="api-input"
            @change="saveApiKey"
          />
        </div>
      </div>
    </Transition>

    <Transition name="fade">
      <div v-if="showPassthroughHint" class="passthrough-hint">
        已开启穿透（Ctrl+Shift+D 恢复）
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'

const SPRITES = {
  idle: '/sprites/idle.png',
  type1: '/sprites/type1.png',
  type2: '/sprites/type2.png',
  burst: '/sprites/burst.png',
  sleep: '/sprites/sleep.png',
} as const

const KEYBOARD_ROWS = [
  ['KeyQ', 'KeyW', 'KeyE', 'KeyR', 'KeyT', 'KeyY', 'KeyU', 'KeyI', 'KeyO', 'KeyP'],
  ['KeyA', 'KeyS', 'KeyD', 'KeyF', 'KeyG', 'KeyH', 'KeyJ', 'KeyK', 'KeyL'],
  ['ShiftLeft', 'KeyZ', 'KeyX', 'KeyC', 'KeyV', 'KeyB', 'KeyN', 'KeyM', 'Backspace'],
  ['ControlLeft', 'AltLeft', 'Space', 'ArrowLeft', 'ArrowDown', 'ArrowRight'],
] as const

type PetVisualState = 'idle' | 'type1' | 'type2' | 'burst' | 'sleep'
type WeekStat = { date: string; keys: number }
type KeyPressPayload = { count: number; key: string }

const todayCount = ref(0)
const recentKeys = ref<number[]>([])
const showPanel = ref(false)
const aiAnalysis = ref('')
const isLoading = ref(false)
const apiKey = ref(localStorage.getItem('claude_api_key') ?? '')
const weekStats = ref<WeekStat[]>([])
const lastKey = ref('')

const isDragging = ref(false)
const passThroughEnabled = ref(false)
const showDebugBorder = ref(false)
const showPassthroughHint = ref(false)
const currentIgnoreState = ref<boolean | null>(null)

const nowTick = ref(Date.now())
const appStartedAt = ref(Date.now())

let passthroughHintTimer: ReturnType<typeof setTimeout> | null = null
let activeKeyTimer: ReturnType<typeof setTimeout> | null = null
let tickTimer: ReturnType<typeof setInterval> | null = null

let unlistenKeyPress: (() => void) | null = null
let unlistenTogglePassThrough: (() => void) | null = null
let unlistenTrayAI: (() => void) | null = null
let unlistenTrayStats: (() => void) | null = null
let unlistenDebugBorder: (() => void) | null = null

const currentWPM = computed(() => {
  const now = nowTick.value
  const recent = recentKeys.value.filter((t) => now - t < 60000)
  return Math.round(recent.length / 5)
})

const weekTotal = computed(() =>
  weekStats.value.reduce((sum, day) => sum + day.keys, 0)
)

const maxKeys = computed(() =>
  Math.max(...weekStats.value.map((d) => d.keys), 1)
)

const shouldForceInteractive = computed(() => {
  return showPanel.value || !!aiAnalysis.value || isDragging.value
})

const recentKeyCount3s = computed(() => {
  const now = nowTick.value
  return recentKeys.value.filter((t) => now - t < 3000).length
})

const petVisualState = computed<PetVisualState>(() => {
  const appUptimeMs = nowTick.value - appStartedAt.value
  const lastKeyTime = recentKeys.value[recentKeys.value.length - 1]

  if (recentKeyCount3s.value >= 20) return 'burst'
  if (recentKeyCount3s.value >= 8) return 'type2'
  if (lastKey.value) return 'type1'

  if (lastKeyTime) {
    const idleMs = nowTick.value - lastKeyTime
    if (idleMs < 280) return 'type1'
    if (idleMs > 20000) return 'sleep'
  } else {
    if (appUptimeMs < 20000) return 'idle'
    return 'sleep'
  }

  return 'idle'
})

const visualStateLabel = computed(() => {
  switch (petVisualState.value) {
    case 'sleep':
      return '休息'
    case 'burst':
      return '爆发'
    case 'type2':
      return '高速打字'
    case 'type1':
      return '打字中'
    default:
      return '待机'
  }
})

const currentFrame = computed(() => {
  switch (petVisualState.value) {
    case 'sleep':
      return SPRITES.sleep
    case 'burst':
      return SPRITES.burst
    case 'type2':
      return SPRITES.type2
    case 'type1':
      return SPRITES.type1
    default:
      return SPRITES.idle
  }
})

function keyLabel(key: string) {
  const map: Record<string, string> = {
    ShiftLeft: 'Shift',
    ShiftRight: 'Shift',
    ControlLeft: 'Ctrl',
    ControlRight: 'Ctrl',
    AltLeft: 'Alt',
    AltRight: 'Alt',
    Space: 'Space',
    Backspace: '⌫',
    Delete: 'Del',
    ArrowLeft: '←',
    ArrowDown: '↓',
    ArrowRight: '→',
    ArrowUp: '↑',
    Unknown: '-',
  }

  if (map[key]) return map[key]
  return key.replace('Key', '').replace('Digit', '')
}

function keyClass(key: string) {
  return {
    active: lastKey.value === key,
    wide: ['ShiftLeft', 'Backspace'].includes(key),
    space: key === 'Space',
    ctrl: key === 'ControlLeft',
    alt: key === 'AltLeft',
  }
}

async function setClickThrough(ignore: boolean) {
  if (currentIgnoreState.value === ignore) return

  try {
    await invoke('set_window_ignore_cursor', { ignore })
    currentIgnoreState.value = ignore
  } catch (err) {
    console.error('set_window_ignore_cursor failed:', err)
  }
}

async function syncWindowMode() {
  const ignore = passThroughEnabled.value && !shouldForceInteractive.value
  await setClickThrough(ignore)
}

async function enablePassThrough() {
  showPanel.value = false
  aiAnalysis.value = ''
  passThroughEnabled.value = true
  await syncWindowMode()

  if (passthroughHintTimer) clearTimeout(passthroughHintTimer)
  showPassthroughHint.value = true
  passthroughHintTimer = setTimeout(() => {
    showPassthroughHint.value = false
  }, 1000)
}

async function disablePassThrough() {
  passThroughEnabled.value = false
  await syncWindowMode()
}

async function togglePassThrough() {
  if (passThroughEnabled.value) {
    await disablePassThrough()
  } else {
    await enablePassThrough()
  }
}

function onKeyPress(count: number, key: string) {
  todayCount.value = count
  lastKey.value = key

  const isRepeatHeavyKey = ['Backspace', 'Delete'].includes(key)

  if (activeKeyTimer) clearTimeout(activeKeyTimer)
  activeKeyTimer = setTimeout(() => {
    lastKey.value = ''
  }, isRepeatHeavyKey ? 80 : 140)

  const now = Date.now()

  if (isRepeatHeavyKey) {
    const lastTime = recentKeys.value[recentKeys.value.length - 1]
    if (!lastTime || now - lastTime > 120) {
      recentKeys.value.push(now)
    }
  } else {
    recentKeys.value.push(now)
  }

  recentKeys.value = recentKeys.value.filter((t) => now - t < 60000)
  nowTick.value = now
}

async function getAIAnalysis() {
  if (isLoading.value) return

  await disablePassThrough()
  isLoading.value = true
  aiAnalysis.value = ''

  try {
    const result = await invoke<string>('analyze_typing', {
      totalKeys: todayCount.value,
      wpm: currentWPM.value,
      apiKey: apiKey.value,
    })
    aiAnalysis.value = result
  } catch (err) {
    console.error(err)
    aiAnalysis.value = `今天打了 ${todayCount.value} 下，狗子觉得你棒棒的 🐾`
  } finally {
    isLoading.value = false
    await syncWindowMode()
  }
}

function saveApiKey() {
  localStorage.setItem('claude_api_key', apiKey.value)
}

function barHeight(keys: number): number {
  return Math.round((keys / maxKeys.value) * 60)
}

function isToday(date: string): boolean {
  const now = new Date()
  const y = now.getFullYear()
  const m = String(now.getMonth() + 1).padStart(2, '0')
  const d = String(now.getDate()).padStart(2, '0')
  return date === `${y}-${m}-${d}`
}

function shortDate(date: string): string {
  const d = new Date(`${date}T00:00:00`)
  return `${d.getMonth() + 1}/${d.getDate()}`
}

async function loadWeekStats() {
  try {
    weekStats.value = await invoke<WeekStat[]>('get_weekly_stats')
  } catch (err) {
    console.error('get_weekly_stats failed:', err)
    weekStats.value = []
  }
}

async function onDogMouseDown(e: MouseEvent) {
  if (passThroughEnabled.value) return

  e.preventDefault()
  e.stopPropagation()

  isDragging.value = true
  await syncWindowMode()

  try {
    await getCurrentWindow().startDragging()
  } catch (err) {
    console.error('startDragging failed:', err)
  } finally {
    isDragging.value = false
    await syncWindowMode()
  }
}

async function openPanel() {
  await disablePassThrough()
  showPanel.value = true
  await loadWeekStats()
}

async function closePanel() {
  showPanel.value = false
  await syncWindowMode()
}

async function closeAI() {
  aiAnalysis.value = ''
  await syncWindowMode()
}

async function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'F8') {
    e.preventDefault()
    await togglePassThrough()
  }

  if (e.key === 'Escape') {
    if (showPanel.value) {
      await closePanel()
      return
    }
    if (aiAnalysis.value) {
      await closeAI()
    }
  }
}

watch(showPanel, async () => {
  await syncWindowMode()
})

watch(aiAnalysis, async () => {
  await syncWindowMode()
})

watch(passThroughEnabled, async () => {
  await syncWindowMode()
})

onMounted(async () => {
  try {
    todayCount.value = await invoke<number>('get_keypress_count')
  } catch (err) {
    console.error('get_keypress_count failed:', err)
  }

  try {
    unlistenKeyPress = await listen<KeyPressPayload>('key-press', (e) => {
      onKeyPress(e.payload.count, e.payload.key)
    })

    unlistenTogglePassThrough = await listen('toggle-pass-through', async () => {
      await togglePassThrough()
    })

    unlistenTrayAI = await listen('tray-ai-analysis', () => getAIAnalysis())
    unlistenTrayStats = await listen('tray-open-stats', () => openPanel())
    unlistenDebugBorder = await listen('toggle-debug-border', () => {
      showDebugBorder.value = !showDebugBorder.value
    })
  } catch (err) {
    console.error('listen failed:', err)
  }

  tickTimer = setInterval(() => {
    nowTick.value = Date.now()
    recentKeys.value = recentKeys.value.filter((t) => nowTick.value - t < 60000)
  }, 200)

  window.addEventListener('keydown', handleKeydown)
  await setClickThrough(false)
})

onUnmounted(() => {
  unlistenKeyPress?.()
  unlistenTogglePassThrough?.()
  unlistenTrayAI?.()
  unlistenTrayStats?.()
  unlistenDebugBorder?.()

  if (activeKeyTimer) clearTimeout(activeKeyTimer)
  if (passthroughHintTimer) clearTimeout(passthroughHintTimer)
  if (tickTimer) clearInterval(tickTimer)

  window.removeEventListener('keydown', handleKeydown)
})
</script>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html,
body {
  background: transparent !important;
  overflow: hidden;
  user-select: none;
  -webkit-user-select: none;
}

* {
  -webkit-user-select: none;
  user-select: none;
}

body {
  font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

.app {
  width: 300px;
  height: 300px;
  position: relative;
  pointer-events: auto;
}

.app.debug-border {
  outline: 2px dashed rgba(255, 80, 80, 0.8);
  outline-offset: -1px;
}

.app.dragging {
  cursor: grabbing;
}

.app.passthrough .ai-bubble,
.app.passthrough .stats-panel,
.app.passthrough .debug-actions,
.app.passthrough .keyboard-dock {
  display: none !important;
}

.panel-mode-btn {
  background: rgba(255, 255, 255, 0.12);
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 8px;
  color: #fff;
  cursor: pointer;
  transition: background 0.15s, transform 0.15s;
  padding: 6px 10px;
  font-size: 11px;
  font-weight: 700;
}

.panel-mode-btn:hover {
  background: rgba(255, 255, 255, 0.2);
  transform: translateY(-1px);
}

.dog-zone {
  position: absolute;
  inset: 0;
  pointer-events: auto;
}

.dog-container {
  position: absolute;
  left: 50%;
  top: 0;
  transform: translateX(-50%);
  width: 240px;
  height: 200px;
  cursor: grab;
  pointer-events: auto;
  transition: transform 0.12s ease;
  z-index: 3;
}

.dog-container:hover {
  transform: translateX(-50%) translateY(-2px);
}

.dog-container.dragging {
  cursor: grabbing;
  transform: translateX(-50%) scale(1.02);
}

.dog-container.typing {
  animation: typingBounce 0.32s ease-in-out infinite;
}

.dog-container.burst {
  animation: burstShake 0.14s ease-in-out infinite;
}

.dog-container.sleeping {
  animation: sleepFloat 2s ease-in-out infinite;
}

.dog-sprite {
  width: 100%;
  height: 100%;
  object-fit: contain;
  image-rendering: auto;
  filter: drop-shadow(0 6px 12px rgba(0, 0, 0, 0.28));
  pointer-events: none;
}

.zzz {
  position: absolute;
  right: 20px;
  top: 10px;
  font-size: 20px;
  font-weight: 800;
  color: rgba(88, 56, 28, 0.92);
  text-shadow: 0 2px 6px rgba(0, 0, 0, 0.18);
  pointer-events: none;
  animation: zFloat 1.4s ease-in-out infinite;
}

.burst-effect {
  position: absolute;
  right: 14px;
  top: 14px;
  font-size: 22px;
  pointer-events: none;
  animation: popFlash 0.28s ease-in-out infinite;
}

.keyboard-dock {
  position: absolute;
  left: 50%;
  bottom: 34px;
  transform: translateX(-50%);
  width: 210px;
  padding: 8px 8px 7px;
  border-radius: 16px;
  background:
    linear-gradient(180deg, rgba(28, 34, 52, 0.92), rgba(12, 14, 22, 0.95));
  border: 1px solid rgba(170, 200, 255, 0.14);
  box-shadow:
    0 10px 28px rgba(0, 0, 0, 0.28),
    inset 0 1px 0 rgba(255, 255, 255, 0.05),
    inset 0 0 24px rgba(98, 147, 255, 0.06);
  backdrop-filter: blur(10px);
  z-index: 2;
  pointer-events: none;
}

.vk-row {
  display: flex;
  justify-content: center;
  gap: 4px;
  margin-bottom: 4px;
}

.vk-row:last-child {
  margin-bottom: 0;
}

.vk-key {
  min-width: 16px;
  height: 20px;
  padding: 0 4px;
  border-radius: 7px;
  background: linear-gradient(
    180deg,
    rgba(255, 255, 255, 0.11),
    rgba(255, 255, 255, 0.04)
  );
  border: 1px solid rgba(255, 255, 255, 0.06);
  color: rgba(235, 240, 255, 0.88);
  font-size: 8px;
  line-height: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow:
    inset 0 -1px 0 rgba(0, 0, 0, 0.2),
    inset 0 1px 0 rgba(255, 255, 255, 0.05);
  transition:
    transform 0.08s ease,
    background 0.12s ease,
    color 0.12s ease,
    box-shadow 0.12s ease;
}

.vk-key.wide {
  min-width: 30px;
}

.vk-key.space {
  min-width: 56px;
}

.vk-key.ctrl,
.vk-key.alt {
  min-width: 26px;
}

.vk-key.active {
  background: linear-gradient(135deg, #7dd3fc, #818cf8);
  border-color: rgba(255, 255, 255, 0.18);
  color: #ffffff;
  transform: translateY(-1px) scale(1.05);
  box-shadow:
    0 0 12px rgba(125, 211, 252, 0.35),
    inset 0 1px 0 rgba(255, 255, 255, 0.18);
}

.debug-actions {
  position: absolute;
  right: 8px;
  bottom: 4px;
  display: flex;
  gap: 6px;
  z-index: 20;
}

.debug-actions button {
  border: none;
  border-radius: 10px;
  padding: 6px 10px;
  font-size: 12px;
  cursor: pointer;
  background: rgba(20, 20, 28, 0.78);
  color: white;
  backdrop-filter: blur(8px);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
}

.debug-actions button:hover {
  transform: translateY(-1px);
  background: rgba(35, 35, 48, 0.9);
}

.ai-bubble {
  position: absolute;
  top: 12px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(255, 255, 255, 0.96);
  border-radius: 14px;
  padding: 10px 30px 10px 12px;
  max-width: 260px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
  z-index: 7;
}

.ai-text {
  font-size: 13px;
  color: #1a1a1a;
  line-height: 1.55;
}

.close-btn {
  position: absolute;
  top: 5px;
  right: 9px;
  background: none;
  border: none;
  font-size: 18px;
  cursor: pointer;
  color: #999;
  line-height: 1;
}

.stats-panel {
  position: absolute;
  inset: 0;
  background: rgba(10, 10, 24, 0.92);
  backdrop-filter: blur(12px);
  border-radius: 16px;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.28);
  z-index: 10;
}

.panel-close {
  position: absolute;
  top: 8px;
  right: 12px;
  color: #888;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-right: 18px;
}

.stats-panel h3 {
  color: #fff;
  font-size: 14px;
  margin-top: 2px;
}

.panel-subtitle {
  color: #bfc5d6;
  font-size: 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.state-tag {
  color: #fff;
  background: rgba(96, 165, 250, 0.18);
  border: 1px solid rgba(96, 165, 250, 0.3);
  border-radius: 999px;
  padding: 2px 8px;
}

.state-meta {
  color: #98a2b3;
}

.week-chart {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 6px;
  flex: 1;
}

.bar-col {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  flex: 1;
}

.bar-wrap {
  height: 64px;
  display: flex;
  align-items: flex-end;
  width: 100%;
}

.bar {
  width: 100%;
  min-height: 3px;
  background: rgba(167, 139, 250, 0.5);
  border-radius: 4px 4px 0 0;
  transition: height 0.3s ease;
}

.bar.today {
  background: linear-gradient(to top, #a78bfa, #60a5fa);
}

.bar-label {
  color: #888;
  font-size: 10px;
}

.week-total {
  color: #bbb;
  font-size: 12px;
  text-align: center;
}

.api-section {
  margin-top: auto;
}

.api-input {
  width: 100%;
  background: rgba(255, 255, 255, 0.07);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 8px;
  padding: 6px 10px;
  color: #ccc;
  font-size: 11px;
  outline: none;
}

.api-input::placeholder {
  color: #555;
}

.api-input:focus {
  border-color: rgba(167, 139, 250, 0.5);
}

.passthrough-hint {
  position: absolute;
  top: 12px;
  left: 50%;
  transform: translateX(-50%);
  font-size: 11px;
  color: rgba(255, 255, 255, 0.9);
  background: rgba(20, 20, 28, 0.58);
  border: 1px solid rgba(255, 255, 255, 0.08);
  padding: 5px 10px;
  border-radius: 999px;
  pointer-events: none;
  backdrop-filter: blur(8px);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.18s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.slide-up-enter-active,
.slide-up-leave-active {
  transition: all 0.25s ease;
}

.slide-up-enter-from {
  opacity: 0;
  transform: translateX(-50%) translateY(10px);
}

.slide-up-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(10px);
}

@keyframes typingBounce {
  0%, 100% {
    transform: translateX(-50%) translateY(0);
  }
  50% {
    transform: translateX(-50%) translateY(-3px);
  }
}

@keyframes burstShake {
  0% {
    transform: translateX(-50%) translate(0, 0) rotate(0deg);
  }
  25% {
    transform: translateX(-50%) translate(-1px, 0) rotate(-1deg);
  }
  50% {
    transform: translateX(-50%) translate(1px, -1px) rotate(1deg);
  }
  75% {
    transform: translateX(-50%) translate(-1px, 1px) rotate(-1deg);
  }
  100% {
    transform: translateX(-50%) translate(0, 0) rotate(0deg);
  }
}

@keyframes sleepFloat {
  0%, 100% {
    transform: translateX(-50%) translateY(0);
  }
  50% {
    transform: translateX(-50%) translateY(2px);
  }
}

@keyframes zFloat {
  0%, 100% {
    transform: translateY(0);
    opacity: 0.75;
  }
  50% {
    transform: translateY(-6px);
    opacity: 1;
  }
}

@keyframes popFlash {
  0%, 100% {
    transform: scale(0.95);
    opacity: 0.8;
  }
  50% {
    transform: scale(1.08);
    opacity: 1;
  }
}
</style>