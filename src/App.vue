<template>
  <div
    class="app"
    :class="{
      dragging: isDragging,
      passthrough: passThroughEnabled,
      'debug-border': showDebugBorder
    }"
  >
    <!-- 狗本体：唯一拖动热区 -->
    <div class="dog-zone">
      <div
        class="dog-container"
        :class="{ dragging: isDragging }"
        @mousedown.left="onDogMouseDown"
        :title="passThroughEnabled ? t.passThroughDragHint : t.dragHint"
      >
        <!-- 底层：没手的狗 + 键盘，始终显示 -->
        <img class="dog-sprite layer-base"
             :src="'/sprites/idle.png'"
             draggable="false" alt="base" />

        <!-- 手部：无键按下时显示 -->
        <img v-if="activeKeys.size === 0"
             class="dog-sprite layer-hand"
             :src="'/sprites/hand.png'"
             draggable="false" alt="" />

        <!-- 按键层：对应键按下时显示 -->
        <template v-for="code in activeKeys" :key="code">
          <img
            v-if="KEY_TO_FILE[code]"
            class="dog-sprite layer-key"
            :src="keySpriteSrc(code)"
            draggable="false"
            :alt="code"
            @error="(e) => ((e.target as HTMLImageElement).style.display = 'none')"
          />
        </template>
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
          <div class="tab-row">
            <button :class="['tab-btn', { active: activeTab === 'week' }]" @click="activeTab = 'week'">{{ t.weekTab }}</button>
            <button :class="['tab-btn', { active: activeTab === 'today' }]" @click="activeTab = 'today'">{{ t.todayTab }}</button>
          </div>
          <button class="panel-mode-btn" @click="enablePassThrough">{{ t.passthroughBtn }}</button>
        </div>

        <!-- 本周 tab -->
        <template v-if="activeTab === 'week'">
          <div class="week-chart">
            <div v-for="day in weekStats" :key="day.date" class="bar-col">
              <div class="bar-wrap">
                <div class="bar" :style="{ height: barHeight(day.keys) + 'px' }" :class="{ today: isToday(day.date) }"></div>
              </div>
              <div class="bar-label">{{ shortDate(day.date) }}</div>
            </div>
          </div>
          <div class="week-total">{{ t.weekTotal(weekTotal) }}</div>
        </template>

        <!-- 今日 tab -->
        <template v-else>
          <div class="today-scroll">
            <!-- WPM 图表 -->
            <div class="section-title">{{ t.wpmTitle }}</div>
            <div v-if="wpmHistory.length === 0" class="no-data">{{ t.noData }}</div>
            <div v-else class="wpm-chart">
              <div v-for="h in wpmHistory" :key="h.hour" class="bar-col">
                <div class="bar-wrap" style="height:48px">
                  <div class="bar today" :style="{ height: wpmBarHeight(h.wpm) + 'px' }"></div>
                </div>
                <div class="bar-label">{{ t.hourLabel(h.hour) }}</div>
              </div>
            </div>

            <!-- 按键频率 -->
            <div class="section-title" style="margin-top:10px">{{ t.keyFreqTitle }}</div>
            <div v-if="keyStats.length === 0" class="no-data">{{ t.noData }}</div>
            <div v-else class="key-stats">
              <div v-for="item in keyStats" :key="item.key" class="key-stat-row">
                <span class="key-label">{{ keyDisplayName(item.key) }}</span>
                <div class="key-bar-wrap">
                  <div class="key-bar" :style="{ width: (item.count / keyStats[0].count * 100) + '%' }"></div>
                </div>
                <span class="key-count">{{ item.count }}</span>
              </div>
            </div>
          </div>
        </template>

      </div>
    </Transition>

    <Transition name="fade">
      <div v-if="showPassthroughHint" class="passthrough-hint">
        {{ t.passthroughHint }}
      </div>
    </Transition>

    <!-- 输入监控权限提示 -->
    <Transition name="fade">
      <div v-if="showAccessibilityAlert" class="accessibility-alert" @mousedown.stop>
        <div class="ax-icon">⌨️</div>
        <div class="ax-title">{{ t.axTitle }}</div>
        <div class="ax-desc">{{ t.axDesc }}</div>
        <button class="ax-btn" @click="openAccessibilitySettings">{{ t.axBtn }}</button>
        <button class="ax-dismiss" @click="showAccessibilityAlert = false">{{ t.axDismiss }}</button>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'

// ── 按键音效 ──────────────────────────────────────────────────────────────────
const audioCtx = new AudioContext()
const clickBuffers: AudioBuffer[] = []
const soundEnabled = ref(true)

async function loadClickSounds() {
  const urls = [
    new URL('./assets/sounds/click1.mp3', import.meta.url).href,
    new URL('./assets/sounds/click2.mp3', import.meta.url).href,
    new URL('./assets/sounds/click3.mp3', import.meta.url).href,
    new URL('./assets/sounds/click4.mp3', import.meta.url).href,
    new URL('./assets/sounds/click5.mp3', import.meta.url).href,
  ]
  for (const url of urls) {
    const resp = await fetch(url)
    const buf = await resp.arrayBuffer()
    clickBuffers.push(await audioCtx.decodeAudioData(buf))
  }
}

function playClick() {
  if (!soundEnabled.value || clickBuffers.length === 0) return
  const buf = clickBuffers[Math.floor(Math.random() * clickBuffers.length)]
  const src = audioCtx.createBufferSource()
  src.buffer = buf
  src.connect(audioCtx.destination)
  src.start()
}
// ─────────────────────────────────────────────────────────────────────────────

type WeekStat = { date: string; keys: number }
type KeyStat = { key: string; count: number }
type WpmPoint = { hour: string; wpm: number; keys: number }
type KeyPressPayload = { count: number; key: string }

// Maps Rust key names → actual sprite filenames (without .png)
const KEY_TO_FILE: Record<string, string> = {
  // Letters
  KeyA: 'A', KeyB: 'B', KeyC: 'C', KeyD: 'D', KeyE: 'E',
  KeyF: 'F', KeyG: 'G', KeyH: 'H', KeyI: 'I', KeyJ: 'J',
  KeyK: 'K', KeyL: 'L', KeyM: 'M', KeyN: 'N', KeyO: 'O',
  KeyP: 'P', KeyQ: 'Q', KeyR: 'R', KeyS: 'S', KeyT: 'T',
  KeyU: 'U', KeyV: 'V', KeyW: 'W', KeyX: 'X', KeyY: 'Y', KeyZ: 'Z',
  // Digits
  Digit0: '0', Digit1: '1', Digit2: '2', Digit3: '3', Digit4: '4',
  Digit5: '5', Digit6: '6', Digit7: '7', Digit8: '8', Digit9: '9',
  // Punctuation
  Comma: ',', Period: 'Dot', Minus: '-', Equal: '+',
  BracketLeft: '[', BracketRight: ']', Backslash: 'backslash', Slash: 'slash',
  Quote: "'", Backquote: '~', Semicolon: ';',
  // Control keys
  Space: 'Space', Return: 'Return', Tab: 'Tab',
  Backspace: 'Delete', Escape: 'Esc', Delete: 'Delete',
  // Modifiers
  ShiftLeft: 'Shift Left', ShiftRight: 'Shift R',
  MetaLeft: 'Command left', MetaRight: 'Command right',
  ControlLeft: 'Control', ControlRight: 'Control',
  Alt: 'Option left', AltRight: 'Option right',
  CapsLock: 'Caps lock',
  // Arrows
  ArrowLeft: 'Left arrow', ArrowRight: 'Right Arrow',
  ArrowDown: 'Down arrow', ArrowUp: 'Up arrow',
  // Function keys
  F1: 'F1', F2: 'F2', F3: 'F3', F4: 'F4', F5: 'F5', F6: 'F6',
  F7: 'F7', F8: 'F8', F9: 'F9', F10: 'F10', F11: 'F11', F12: 'F12',
}

function keySpriteSrc(code: string): string {
  const filename = KEY_TO_FILE[code]
  if (!filename) return '/sprites/keyboard/_none.png'
  return `/sprites/keyboard/${encodeURIComponent(filename)}.png`
}

const todayCount = ref(0)
const recentKeys = ref<number[]>([])
const showPanel = ref(false)
const activeTab = ref<'week' | 'today'>('week')
const keyStats = ref<KeyStat[]>([])
const wpmHistory = ref<WpmPoint[]>([])
const aiAnalysis = ref('')
const isLoading = ref(false)
const apiKey = ref(localStorage.getItem('claude_api_key') ?? '')
const weekStats = ref<WeekStat[]>([])

const activeKeys = ref<Set<string>>(new Set())

const isDragging = ref(false)
const passThroughEnabled = ref(false)
const showDebugBorder = ref(false)
const showPassthroughHint = ref(false)
const showAccessibilityAlert = ref(false)
let passthroughHintTimer: ReturnType<typeof setTimeout> | null = null
const currentIgnoreState = ref<boolean | null>(null)

let unlistenKeyPress: (() => void) | null = null
let unlistenKeyRelease: (() => void) | null = null
let unlistenTogglePassThrough: (() => void) | null = null
let unlistenTrayAI: (() => void) | null = null
let unlistenTrayStats: (() => void) | null = null


const lang = ref(localStorage.getItem('lang') ?? 'zh') // will be synced from Rust on mount

const t = computed(() => {
  const zh = {
    dragHint: '拖动我',
    passThroughDragHint: '穿透模式：无法拖动',
    weekTab: '本周',
    todayTab: '今日',
    passthroughBtn: '🖱 穿透模式',
    weekTotal: (n: number) => `本周共击键 ${n} 次`,
    wpmTitle: '打字速度 (WPM)',
    noData: '今日暂无数据',
    hourLabel: (h: string) => h,
    keyFreqTitle: '按键频率',
    passthroughHint: '已开启穿透（Ctrl+Shift+D 恢复）',
    axTitle: '需要「输入监控」权限',
    axDesc: '系统设置 → 隐私与安全性 → 输入监控，添加本应用后重启',
    axBtn: '去授权',
    axDismiss: '稍后',
  }
  const en = {
    dragHint: 'Drag me',
    passThroughDragHint: 'Pass-through: cannot drag',
    weekTab: 'Week',
    todayTab: 'Today',
    passthroughBtn: '🖱 Pass-through',
    weekTotal: (n: number) => `${n} keystrokes this week`,
    wpmTitle: 'Typing Speed (WPM)',
    noData: 'No data yet today',
    hourLabel: (h: string) => h,
    keyFreqTitle: 'Key Frequency',
    passthroughHint: 'Pass-through enabled (Ctrl+Shift+D to exit)',
    axTitle: 'Input Monitoring Required',
    axDesc: 'System Settings → Privacy & Security → Input Monitoring, add this app then restart',
    axBtn: 'Authorize',
    axDismiss: 'Later',
  }
  return lang.value === 'en' ? en : zh
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
  playClick()
  todayCount.value = count
  const next = new Set(activeKeys.value)
  next.add(key)
  activeKeys.value = next
  const now = Date.now()
  recentKeys.value.push(now)
  recentKeys.value = recentKeys.value.filter((t) => now - t < 60000)
}

function onKeyRelease(key: string) {
  const s = new Set(activeKeys.value)
  s.delete(key)
  activeKeys.value = s
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
      lang: lang.value,
    })
    aiAnalysis.value = result
  } catch (err) {
    console.error(err)
    aiAnalysis.value = lang.value === 'en'
      ? `${todayCount.value} keystrokes today — Yamper thinks you're awesome 🐾`
      : `今天打了 ${todayCount.value} 下，Yamper 觉得你棒棒的 🐾`
  } finally {
    isLoading.value = false
    await syncWindowMode()
  }
}


function barHeight(keys: number): number {
  return Math.round((keys / maxKeys.value) * 60)
}

const maxWpm = computed(() => Math.max(...wpmHistory.value.map(h => h.wpm), 1))

function wpmBarHeight(wpm: number): number {
  return Math.round((wpm / maxWpm.value) * 48)
}

function keyDisplayName(key: string): string {
  if (key.startsWith('Key')) return key.slice(3)
  if (key.startsWith('Digit')) return key.slice(5)
  const map: Record<string, string> = {
    Space: '␣', Return: '↵', Backspace: '⌫', Tab: '⇥',
    Escape: 'Esc', Delete: 'Del',
    ShiftLeft: '⇧L', ShiftRight: '⇧R',
    MetaLeft: '⌘L', MetaRight: '⌘R',
    Alt: '⌥L', AltRight: '⌥R',
    ControlLeft: '⌃L', ControlRight: '⌃R',
    CapsLock: '⇪', Fn: 'Fn',
    ArrowLeft: '←', ArrowRight: '→', ArrowUp: '↑', ArrowDown: '↓',
    Comma: ',', Period: '.', Slash: '/', Backslash: '\\',
    Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']',
    Quote: "'", Backquote: '`', Semicolon: ';',
  }
  return map[key] ?? key
}

async function loadTodayStats() {
  try {
    const [ks, wpm] = await Promise.all([
      invoke<KeyStat[]>('get_key_stats'),
      invoke<WpmPoint[]>('get_wpm_history'),
    ])
    keyStats.value = ks
    wpmHistory.value = wpm
  } catch (err) {
    console.error('loadTodayStats failed:', err)
  }
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
  await Promise.all([loadWeekStats(), loadTodayStats()])
}

async function closePanel() {
  showPanel.value = false
  await syncWindowMode()
}

async function closeAI() {
  aiAnalysis.value = ''
  await syncWindowMode()
}

async function openAccessibilitySettings() {
  await invoke('open_accessibility_settings')
  showAccessibilityAlert.value = false
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
  loadClickSounds().catch(console.error)

  try {
    soundEnabled.value = await invoke<boolean>('get_sound')
  } catch (err) {
    console.error('get_sound failed:', err)
  }

  try {
    const savedLang = await invoke<string>('get_language')
    lang.value = savedLang
    localStorage.setItem('lang', savedLang)
  } catch (err) {
    console.error('get_language failed:', err)
  }

  try {
    const trusted = await invoke<boolean>('check_accessibility')
    if (!trusted) showAccessibilityAlert.value = true
  } catch (err) {
    console.error('check_accessibility failed:', err)
  }

  try {
    todayCount.value = await invoke<number>('get_keypress_count')
  } catch (err) {
    console.error('get_keypress_count failed:', err)
  }

  try {
    unlistenKeyPress = await listen<KeyPressPayload>('key-press', (e) => {
      onKeyPress(e.payload.count, e.payload.key)
    })
    unlistenKeyRelease = await listen<string>('key-release', (e) => {
      onKeyRelease(e.payload)
    })

    unlistenTogglePassThrough = await listen('toggle-pass-through', async () => {
      await togglePassThrough()
    })

    unlistenTrayAI = await listen('tray-ai-analysis', () => getAIAnalysis())
    unlistenTrayStats = await listen('tray-open-stats', () => openPanel())
    await listen('toggle-debug-border', () => { showDebugBorder.value = !showDebugBorder.value })
    await listen<string>('set-language', (e) => {
      lang.value = e.payload
      localStorage.setItem('lang', e.payload)
    })
    await listen<boolean>('set-sound', (e) => {
      soundEnabled.value = e.payload
    })
  } catch (err) {
    console.error('listen failed:', err)
  }

  await setClickThrough(false)
})

onUnmounted(() => {
  unlistenKeyPress?.()
  unlistenKeyRelease?.()
  unlistenTogglePassThrough?.()
  unlistenTrayAI?.()
  unlistenTrayStats?.()

  if (passthroughHintTimer) clearTimeout(passthroughHintTimer)
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
.app.passthrough .stats-panel {
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
  inset: 0;
  width: 300px;
  height: 300px;
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

.layer-base,
.layer-hand,
.layer-key {
  position: absolute;
  inset: 0;
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
  margin-right: 20px;
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

.tab-row {
  display: flex;
  gap: 4px;
}

.tab-btn {
  flex: 1;
  background: rgba(255, 255, 255, 0.07);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  color: #888;
  cursor: pointer;
  font-size: 11px;
  font-weight: 600;
  padding: 4px 0;
  transition: all 0.15s;
}

.tab-btn.active {
  background: rgba(167, 139, 250, 0.25);
  border-color: rgba(167, 139, 250, 0.5);
  color: #c4b5fd;
}

.today-scroll {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}

.today-scroll::-webkit-scrollbar {
  width: 3px;
}

.today-scroll::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.15);
  border-radius: 2px;
}

.section-title {
  color: #999;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: 6px;
}

.no-data {
  color: #555;
  font-size: 11px;
  text-align: center;
  padding: 8px 0;
}

.wpm-chart {
  display: flex;
  align-items: flex-end;
  gap: 3px;
  margin-bottom: 4px;
}

.wpm-chart .bar-col {
  flex: 1;
  min-width: 0;
}

.wpm-chart .bar-label {
  font-size: 9px;
}

.key-stats {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.key-stat-row {
  display: flex;
  align-items: center;
  gap: 5px;
}

.key-label {
  color: #bbb;
  font-size: 10px;
  font-family: monospace;
  width: 28px;
  flex-shrink: 0;
  text-align: right;
}

.key-bar-wrap {
  flex: 1;
  height: 8px;
  background: rgba(255, 255, 255, 0.06);
  border-radius: 2px;
  overflow: hidden;
}

.key-bar {
  height: 100%;
  background: linear-gradient(to right, #a78bfa, #60a5fa);
  border-radius: 2px;
  min-width: 2px;
  transition: width 0.3s ease;
}

.key-count {
  color: #666;
  font-size: 9px;
  width: 32px;
  flex-shrink: 0;
  text-align: right;
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


.accessibility-alert {
  position: absolute;
  inset: 0;
  background: rgba(10, 10, 24, 0.94);
  backdrop-filter: blur(12px);
  border-radius: 16px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 24px 20px;
  z-index: 20;
  text-align: center;
}

.ax-icon {
  font-size: 32px;
}

.ax-title {
  color: #fff;
  font-size: 14px;
  font-weight: 700;
}

.ax-desc {
  color: #aaa;
  font-size: 11px;
  line-height: 1.5;
}

.ax-btn {
  margin-top: 8px;
  background: linear-gradient(135deg, #a78bfa, #60a5fa);
  border: none;
  border-radius: 8px;
  color: #fff;
  cursor: pointer;
  font-size: 12px;
  font-weight: 700;
  padding: 7px 18px;
}

.ax-btn:hover {
  opacity: 0.88;
}

.ax-dismiss {
  background: none;
  border: none;
  color: #666;
  cursor: pointer;
  font-size: 11px;
}

.ax-dismiss:hover {
  color: #999;
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