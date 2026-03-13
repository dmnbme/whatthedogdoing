<template>
  <div
    class="app"
    :class="{
      dragging: isDragging,
      passthrough: passThroughEnabled
    }"
  >
    <!-- 同一个 hover 区：覆盖顶部面板 + 狗 -->
    <div
      class="hover-zone"
      @mouseenter="onHoverZoneEnter"
      @mouseleave="onHoverZoneLeave"
    >
      <!-- 顶部小统计 -->
      <Transition name="fade">
        <div
          v-if="showStats && !showPanel && !passThroughEnabled"
          class="stats-overlay"
          @mousedown.stop
        >
          <div class="stat-row">⌨️ <span>{{ todayCount.toLocaleString() }}</span></div>
          <div class="stat-row">⚡ <span>{{ currentWPM }} WPM</span></div>

          <div class="shortcut-hint">
            穿透快捷键：Ctrl+Shift+D
          </div>

          <div class="btn-col">
            <button class="ai-btn" @click="getAIAnalysis" :disabled="isLoading">
              {{ isLoading ? '...' : '✨ what the dog doin?' }}
            </button>

            <div class="btn-row">
              <button class="icon-btn" @click="openPanel" title="Stats">📊 详情</button>
              <button class="mode-btn" @click="enablePassThrough" title="开启鼠标穿透">
                🫥 穿透
              </button>
            </div>
          </div>
        </div>
      </Transition>

      <!-- 狗本体：唯一拖动热区 -->
      <div class="dog-zone">
        <div
          class="dog-container"
          :class="[petVisualState, { dragging: isDragging }]"
          @mousedown.left="onDogMouseDown"
          :title="passThroughEnabled ? '当前是穿透模式，按 Ctrl+Shift+D 恢复交互' : '按住可以拖动'"
        >
          <div v-if="USE_PLACEHOLDER" class="dog-placeholder-wrap">
            <div class="dog-placeholder">🐕</div>
            <div v-if="petVisualState === 'typing'" class="typing-badge">⌨️</div>
            <div v-if="petVisualState === 'burst'" class="typing-badge burst-badge">⚡</div>
          </div>

          <img
            v-else
            :src="currentFrame"
            class="dog-sprite"
            draggable="false"
            alt="yamper"
          />

          <div v-if="petVisualState === 'burst'" class="burst-effect">💥</div>
          <div v-if="petVisualState === 'sleep'" class="zzz">z z z</div>
        </div>

        <!-- 狗正下方：紧凑虚拟键盘 -->
        <Transition name="fade">
          <div
            v-if="normalizedActiveKey"
            class="keyboard-dock"
          >
            <div
              v-for="(row, rowIndex) in keyboardRows"
              :key="rowIndex"
              class="vk-row"
            >
              <div
                v-for="key in row"
                :key="key.code"
                class="vk-key"
                :class="[
                  key.size || '',
                  { active: isKeyActive(key) }
                ]"
              >
                {{ key.label }}
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>

    <!-- AI 气泡 -->
    <Transition name="slide-up">
      <div
        v-if="aiAnalysis && !showPanel && !passThroughEnabled"
        class="ai-bubble"
      >
        <div class="ai-text">{{ aiAnalysis }}</div>
        <button class="close-btn" @click="closeAI">×</button>
      </div>
    </Transition>

    <!-- 详情面板 -->
    <Transition name="fade">
      <div v-if="showPanel" class="stats-panel" @mousedown.stop>
        <button class="close-btn panel-close" @click="closePanel">×</button>

        <div class="panel-header">
          <h3>📊 本周战绩</h3>
          <button class="panel-mode-btn" @click="enablePassThrough">
            开启穿透
          </button>
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
      <div v-if="passThroughEnabled" class="passthrough-hint">
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

const USE_PLACEHOLDER = false

const SPRITES = {
  idle: '/sprites/idle.png',
  type1: '/sprites/type1.png',
  type2: '/sprites/type2.png',
  burst: '/sprites/burst.png',
  sleep: '/sprites/sleep.png',
}

type DogState = 'idle' | 'typing' | 'burst' | 'sleep'
type WeekStat = { date: string; keys: number }
type KeyPressPayload = { count: number; key: string }
type KeySize = 'wide' | 'space'

type KeyboardKey = {
  code: string
  label: string
  aliases?: string[]
  size?: KeySize
}

const keyboardRows: KeyboardKey[][] = [
  [
    { code: 'KeyQ', label: 'Q' },
    { code: 'KeyW', label: 'W' },
    { code: 'KeyE', label: 'E' },
    { code: 'KeyR', label: 'R' },
    { code: 'KeyT', label: 'T' },
    { code: 'KeyY', label: 'Y' },
    { code: 'KeyU', label: 'U' },
    { code: 'KeyI', label: 'I' },
    { code: 'KeyO', label: 'O' },
    { code: 'KeyP', label: 'P' },
  ],
  [
    { code: 'KeyA', label: 'A' },
    { code: 'KeyS', label: 'S' },
    { code: 'KeyD', label: 'D' },
    { code: 'KeyF', label: 'F' },
    { code: 'KeyG', label: 'G' },
    { code: 'KeyH', label: 'H' },
    { code: 'KeyJ', label: 'J' },
    { code: 'KeyK', label: 'K' },
    { code: 'KeyL', label: 'L' },
    { code: 'Enter', label: '⏎', aliases: ['Return'], size: 'wide' },
  ],
  [
    { code: 'ShiftLeft', label: 'Shift', aliases: ['ShiftRight'], size: 'wide' },
    { code: 'KeyZ', label: 'Z' },
    { code: 'KeyX', label: 'X' },
    { code: 'KeyC', label: 'C' },
    { code: 'KeyV', label: 'V' },
    { code: 'KeyB', label: 'B' },
    { code: 'KeyN', label: 'N' },
    { code: 'KeyM', label: 'M' },
    { code: 'Backspace', label: '⌫', size: 'wide' },
  ],
  [
    { code: 'ControlLeft', label: 'Ctrl', aliases: ['ControlRight'], size: 'wide' },
    { code: 'Alt', label: 'Alt', aliases: ['AltGr'], size: 'wide' },
    { code: 'Space', label: 'Space', aliases: ['Spacebar'], size: 'space' },
    { code: 'ArrowLeft', label: '←' },
    { code: 'ArrowUp', label: '↑' },
    { code: 'ArrowDown', label: '↓' },
    { code: 'ArrowRight', label: '→' },
  ],
]

const dogState = ref<DogState>('idle')
const frameToggle = ref(false)
const todayCount = ref(0)
const recentKeys = ref<number[]>([])
const showStats = ref(false)
const showPanel = ref(false)
const aiAnalysis = ref('')
const isLoading = ref(false)
const apiKey = ref(localStorage.getItem('claude_api_key') ?? '')
const weekStats = ref<WeekStat[]>([])
const lastKey = ref('')

const isDragging = ref(false)
const isPointerInside = ref(false)
const passThroughEnabled = ref(false)
const currentIgnoreState = ref<boolean | null>(null)
const lastBurstAt = ref(0)

let idleTimer: ReturnType<typeof setTimeout> | null = null
let burstCooldownTimer: ReturnType<typeof setTimeout> | null = null
let activeKeyTimer: ReturnType<typeof setTimeout> | null = null
let unlistenKeyPress: (() => void) | null = null
let unlistenTogglePassThrough: (() => void) | null = null

const nonBurstKeys = new Set([
  'Backspace',
  'Delete',
  'ShiftLeft',
  'ShiftRight',
  'ControlLeft',
  'ControlRight',
  'Alt',
  'AltGr',
  'MetaLeft',
  'MetaRight',
  'CapsLock',
  'ArrowLeft',
  'ArrowRight',
  'ArrowUp',
  'ArrowDown',
  'Escape',
  'Tab',
  'Unknown',
])

const petVisualState = computed<DogState | 'dragging'>(() => {
  if (isDragging.value) return 'dragging'
  return dogState.value
})

const currentFrame = computed(() => {
  switch (petVisualState.value) {
    case 'sleep':
      return SPRITES.sleep
    case 'burst':
      return SPRITES.burst
    case 'typing':
      return frameToggle.value ? SPRITES.type1 : SPRITES.type2
    default:
      return SPRITES.idle
  }
})

const currentWPM = computed(() => {
  const now = Date.now()
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

function normalizeKey(key: string): string {
  const trimmed = (key || '').trim()

  const exactMap: Record<string, string> = {
    Return: 'Enter',
    Spacebar: 'Space',
    ControlRight: 'ControlLeft',
    ShiftRight: 'ShiftLeft',
    AltGr: 'Alt',
  }

  if (exactMap[trimmed]) return exactMap[trimmed]
  if (/^Key[A-Z]$/.test(trimmed)) return trimmed
  if (/^Digit[0-9]$/.test(trimmed)) return trimmed
  if (/^Num[0-9]$/.test(trimmed)) return `Digit${trimmed.replace('Num', '')}`
  if (/^[A-Z]$/.test(trimmed)) return `Key${trimmed}`
  if (/^[0-9]$/.test(trimmed)) return `Digit${trimmed}`

  return trimmed
}

const normalizedActiveKey = computed(() => normalizeKey(lastKey.value))

function isKeyActive(key: KeyboardKey): boolean {
  const active = normalizedActiveKey.value
  if (!active) return false
  if (key.code === active) return true
  return !!key.aliases?.includes(active)
}

function displayKey(key: string): string {
  if (!key) return ''

  const map: Record<string, string> = {
    Backspace: '⌫',
    Delete: 'Del',
    Space: 'Space',
    Enter: 'Enter',
    Return: 'Enter',
    Tab: 'Tab',
    ShiftLeft: 'Shift',
    ShiftRight: 'Shift',
    ControlLeft: 'Ctrl',
    ControlRight: 'Ctrl',
    Alt: 'Alt',
    AltGr: 'AltGr',
    MetaLeft: 'Win',
    MetaRight: 'Win',
    CapsLock: 'Caps',
    ArrowLeft: '←',
    ArrowRight: '→',
    ArrowUp: '↑',
    ArrowDown: '↓',
    Escape: 'Esc',
  }

  if (map[key]) return map[key]
  if (/^Key[A-Z]$/.test(key)) return key.replace('Key', '')
  if (/^Digit[0-9]$/.test(key)) return key.replace('Digit', '')
  return key
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
  showStats.value = false
  showPanel.value = false
  aiAnalysis.value = ''
  passThroughEnabled.value = true
  await syncWindowMode()
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

  if (activeKeyTimer) clearTimeout(activeKeyTimer)
  activeKeyTimer = setTimeout(() => {
    lastKey.value = ''
  }, 420)

  const now = Date.now()
  recentKeys.value.push(now)
  recentKeys.value = recentKeys.value.filter((t) => now - t < 60000)

  const lastSecond = recentKeys.value.filter((t) => now - t < 1000).length
  const normalizedKey = normalizeKey(key)
  const isEditingKey = nonBurstKeys.has(normalizedKey)

  dogState.value = 'typing'
  frameToggle.value = !frameToggle.value

  if (!isEditingKey && lastSecond > 12 && now - lastBurstAt.value > 1400) {
    dogState.value = 'burst'
    lastBurstAt.value = now

    if (burstCooldownTimer) clearTimeout(burstCooldownTimer)
    burstCooldownTimer = setTimeout(() => {
      dogState.value = 'typing'
    }, 140)
  }

  if (idleTimer) clearTimeout(idleTimer)
  idleTimer = setTimeout(() => {
    dogState.value = 'idle'
    idleTimer = setTimeout(() => {
      dogState.value = 'sleep'
    }, 30000)
  }, 1200)
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
    aiAnalysis.value = `今天打了 ${todayCount.value} 下，Yamper 觉得你棒棒的 🐾`
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

  showStats.value = false
  isDragging.value = true
  await syncWindowMode()

  try {
    await getCurrentWindow().startDragging()
  } catch (err) {
    console.error('startDragging failed:', err)
  } finally {
    isDragging.value = false

    if (isPointerInside.value && !showPanel.value && !passThroughEnabled.value) {
      showStats.value = true
    }

    await syncWindowMode()
  }
}

async function onHoverZoneEnter() {
  isPointerInside.value = true
  if (passThroughEnabled.value) return
  showStats.value = true
  await loadWeekStats()
}

function onHoverZoneLeave() {
  isPointerInside.value = false
  if (passThroughEnabled.value) return
  if (!showPanel.value) {
    showStats.value = false
  }
}

async function openPanel() {
  await disablePassThrough()
  showPanel.value = true
  showStats.value = false
  await loadWeekStats()
}

async function closePanel() {
  showPanel.value = false
  if (isPointerInside.value && !passThroughEnabled.value) {
    showStats.value = true
  }
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

watch(passThroughEnabled, async (enabled) => {
  if (enabled) {
    showStats.value = false
  }
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
  } catch (err) {
    console.error('listen failed:', err)
  }

  await setClickThrough(false)
})

onUnmounted(() => {
  unlistenKeyPress?.()
  unlistenTogglePassThrough?.()

  if (idleTimer) clearTimeout(idleTimer)
  if (burstCooldownTimer) clearTimeout(burstCooldownTimer)
  if (activeKeyTimer) clearTimeout(activeKeyTimer)
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

.app.dragging {
  cursor: grabbing;
}

.app.passthrough .stats-overlay,
.app.passthrough .ai-bubble,
.app.passthrough .stats-panel {
  display: none !important;
}

/* 整个 hover 区：覆盖顶部面板到狗本体 */
.hover-zone {
  position: absolute;
  left: 50%;
  top: 18px;
  transform: translateX(-50%);
  width: 220px;
  height: 290px;
  pointer-events: auto;
}

/* 顶部 stats */
.stats-overlay {
  position: absolute;
  top: 0;
  left: 50%;
  transform: translateX(-50%);
  width: 198px;
  background: rgba(10, 10, 20, 0.82);
  backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 14px;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  box-shadow: 0 10px 26px rgba(0, 0, 0, 0.22);
  z-index: 5;
  pointer-events: auto;
}

.stat-row {
  color: #e0e0e0;
  font-size: 12px;
  display: flex;
  justify-content: space-between;
  gap: 8px;
}

.stat-row span {
  color: #fff;
  font-weight: 700;
}

.shortcut-hint {
  font-size: 10px;
  color: #aab4d9;
  text-align: center;
  background: rgba(255, 255, 255, 0.05);
  border-radius: 8px;
  padding: 4px 6px;
}

.btn-col {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 2px;
}

.btn-row {
  display: flex;
  gap: 6px;
}

.ai-btn {
  width: 100%;
  background: linear-gradient(135deg, #a78bfa, #60a5fa);
  border: none;
  border-radius: 8px;
  color: white;
  font-size: 11px;
  padding: 6px 8px;
  cursor: pointer;
  font-weight: 700;
  transition: opacity 0.15s, transform 0.15s;
  white-space: nowrap;
}

.ai-btn:hover {
  opacity: 0.9;
  transform: translateY(-1px);
}

.ai-btn:disabled {
  opacity: 0.45;
  cursor: default;
  transform: none;
}

.icon-btn,
.mode-btn,
.panel-mode-btn {
  flex: 1;
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

.icon-btn:hover,
.mode-btn:hover,
.panel-mode-btn:hover {
  background: rgba(255, 255, 255, 0.2);
  transform: translateY(-1px);
}

/* 狗固定在 hover 区底部 */
.dog-zone {
  position: absolute;
  left: 50%;
  bottom: 0;
  transform: translateX(-50%);
  width: 200px;
  height: 240px;
  pointer-events: auto;
}

.dog-container {
  position: absolute;
  left: 0;
  bottom: 40px;
  width: 200px;
  height: 182px;
  cursor: grab;
  pointer-events: auto;
  transition: transform 0.12s ease;
  z-index: 3;
}

.dog-container:hover {
  transform: translateY(-2px);
}

.dog-container.dragging {
  cursor: grabbing;
  transform: scale(1.02);
}

.dog-sprite {
  width: 100%;
  height: 100%;
  image-rendering: pixelated;
  filter: drop-shadow(0 6px 12px rgba(0, 0, 0, 0.28));
  pointer-events: none;
}

.dog-placeholder-wrap {
  width: 100%;
  height: 100%;
  position: relative;
}

.dog-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 80px;
  line-height: 1;
  filter: drop-shadow(0 6px 12px rgba(0, 0, 0, 0.28));
  pointer-events: none;
}

.typing-badge {
  position: absolute;
  right: 14px;
  bottom: 26px;
  font-size: 24px;
  line-height: 1;
  pointer-events: none;
  animation: badge-pulse 0.45s ease-in-out infinite alternate;
}

.burst-badge {
  animation: burst-pulse 0.18s ease-in-out infinite alternate;
}

.dog-container.typing .dog-placeholder,
.dog-container.typing .dog-sprite {
  animation: breathe-typing 0.22s ease-in-out infinite alternate;
}

.dog-container.burst .dog-sprite,
.dog-container.burst .dog-placeholder {
  animation: shake 0.08s linear infinite;
  filter: drop-shadow(0 0 14px #ffdd00) drop-shadow(0 6px 12px rgba(0, 0, 0, 0.28));
}

.dog-container.sleep .dog-sprite,
.dog-container.sleep .dog-placeholder {
  opacity: 0.86;
  animation: breathe 3s ease-in-out infinite;
}

/* 狗正下方虚拟键盘 */
.keyboard-dock {
  position: absolute;
  left: 50%;
  bottom: 0;
  transform: translateX(-50%);
  width: 196px;
  padding: 10px 8px 8px;
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

.keyboard-dock::before {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: 16px;
  padding: 1px;
  background: linear-gradient(
    135deg,
    rgba(120, 170, 255, 0.22),
    rgba(255, 255, 255, 0.03)
  );
  -webkit-mask:
    linear-gradient(#fff 0 0) content-box,
    linear-gradient(#fff 0 0);
  -webkit-mask-composite: xor;
  mask-composite: exclude;
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
  min-width: 15px;
  height: 18px;
  padding: 0 4px;
  border-radius: 6px;
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
  min-width: 58px;
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

.zzz {
  position: absolute;
  top: -8px;
  right: -12px;
  color: #fff;
  font-size: 12px;
  font-weight: 700;
  animation: float-up 2s ease-in-out infinite;
  text-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
  letter-spacing: 3px;
  pointer-events: none;
}

.burst-effect {
  position: absolute;
  top: 0;
  right: 0;
  font-size: 28px;
  animation: pop 0.3s ease-out forwards;
  pointer-events: none;
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

@keyframes breathe-typing {
  from { transform: translateY(0) scale(1); }
  to { transform: translateY(-1px) scale(1.005); }
}

@keyframes badge-pulse {
  from { transform: scale(0.98); opacity: 0.82; }
  to { transform: scale(1.04); opacity: 1; }
}

@keyframes burst-pulse {
  from { transform: scale(0.97); opacity: 0.8; }
  to { transform: scale(1.08); opacity: 1; }
}

@keyframes shake {
  0% { transform: translateX(-2px) rotate(-1deg); }
  50% { transform: translateX(2px) rotate(1deg); }
  100% { transform: translateX(-2px) rotate(-1deg); }
}

@keyframes breathe {
  0%, 100% { transform: scaleY(1); }
  50% { transform: scaleY(0.97); }
}

@keyframes pop {
  0% { transform: scale(0); opacity: 1; }
  100% { transform: scale(1.5); opacity: 0; }
}

@keyframes float-up {
  0%, 100% { transform: translateY(0); opacity: 1; }
  50% { transform: translateY(-8px); opacity: 0.6; }
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
</style>