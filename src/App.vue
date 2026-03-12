<template>
  <div class="app" @mouseenter="onMouseEnter" @mouseleave="onMouseLeave">
    <!-- Dog sprite — mousedown starts drag when not hovering stats -->
    <div
      class="dog-container"
      :class="dogState"
      @mousedown.left="onDogMouseDown"
    >
      <div v-if="USE_PLACEHOLDER" class="dog-placeholder">{{ placeholderEmoji }}</div>
      <img v-else :src="currentFrame" class="dog-sprite" draggable="false" alt="yamper" />
      <div v-if="dogState === 'burst'" class="burst-effect">💥</div>
      <div v-if="dogState === 'sleep'" class="zzz">z z z</div>
    </div>

    <!-- Stats overlay (show on hover) — stop mousedown to prevent drag -->
    <Transition name="fade">
      <div v-if="showStats && !showPanel" class="stats-overlay" @mousedown.stop>
        <div class="stat-row">⌨️ <span>{{ todayCount.toLocaleString() }}</span></div>
        <div class="stat-row">⚡ <span>{{ currentWPM }} WPM</span></div>
        <div class="btn-row">
          <button class="ai-btn" @click="getAIAnalysis" :disabled="isLoading">
            {{ isLoading ? '...' : '✨ what the dog doin?' }}
          </button>
          <button class="icon-btn" @click="showPanel = true" title="Stats">📊</button>
        </div>
      </div>
    </Transition>

    <!-- AI analysis bubble -->
    <Transition name="slide-up">
      <div v-if="aiAnalysis && !showPanel" class="ai-bubble" @mousedown.stop>
        <div class="ai-text">{{ aiAnalysis }}</div>
        <button class="close-btn" @click="aiAnalysis = ''">×</button>
      </div>
    </Transition>

    <!-- Stats panel (full overlay) — stop mousedown to prevent drag -->
    <Transition name="fade">
      <div v-if="showPanel" class="stats-panel" @mousedown.stop>
        <button class="close-btn panel-close" @click="showPanel = false">×</button>
        <h3>📊 本周战绩</h3>
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
            placeholder="Claude API Key (可选)"
            class="api-input"
            @change="saveApiKey"
          />
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'

// ── Sprites ────────────────────────────────────────────────────────────────
// Set USE_PLACEHOLDER to true until real sprites are ready
const USE_PLACEHOLDER = true

const SPRITES = {
  idle:  '/sprites/idle.png',
  type1: '/sprites/type1.png',
  type2: '/sprites/type2.png',
  burst: '/sprites/burst.png',
  sleep: '/sprites/sleep.png',
}

// ── State ──────────────────────────────────────────────────────────────────
type DogState = 'idle' | 'typing' | 'burst' | 'sleep'

const dogState    = ref<DogState>('idle')
const frameToggle = ref(false)
const todayCount  = ref(0)
const recentKeys  = ref<number[]>([])
const showStats   = ref(false)
const showPanel   = ref(false)
const aiAnalysis  = ref('')
const isLoading   = ref(false)
const apiKey      = ref(localStorage.getItem('claude_api_key') ?? '')
const weekStats = ref<{ date: string; keys: number }[]>([])

let idleTimer: ReturnType<typeof setTimeout> | null = null
let frameTimer: ReturnType<typeof setInterval> | null = null
let unlisten: (() => void) | null = null

// ── Computed ───────────────────────────────────────────────────────────────
const currentFrame = computed(() => {
  switch (dogState.value) {
    case 'sleep':  return SPRITES.sleep
    case 'burst':  return SPRITES.burst
    case 'typing': return frameToggle.value ? SPRITES.type1 : SPRITES.type2
    default:       return SPRITES.idle
  }
})

const placeholderEmoji = computed(() => {
  switch (dogState.value) {
    case 'sleep':  return '😴'
    case 'burst':  return '🐕💨'
    case 'typing': return frameToggle.value ? '🐕⌨️' : '🐾⌨️'
    default:       return '🐕'
  }
})

const currentWPM = computed(() => {
  const now = Date.now()
  const recent = recentKeys.value.filter(t => now - t < 60000)
  return Math.round(recent.length / 5)
})

const weekTotal = computed(() =>
  weekStats.value.reduce((s, d) => s + d.keys, 0)
)

const maxKeys = computed(() =>
  Math.max(...weekStats.value.map(d => d.keys), 1)
)

// ── Logic ──────────────────────────────────────────────────────────────────
function onKeyPress(count: number) {
  todayCount.value = count
  recentKeys.value.push(Date.now())
  recentKeys.value = recentKeys.value.filter(t => Date.now() - t < 60000)

  const lastSecond = recentKeys.value.filter(t => Date.now() - t < 1000)
  dogState.value = lastSecond.length > 8 ? 'burst' : 'typing'

  if (idleTimer) clearTimeout(idleTimer)
  idleTimer = setTimeout(() => {
    dogState.value = 'idle'
    idleTimer = setTimeout(() => { dogState.value = 'sleep' }, 30000)
  }, 1500)
}

async function getAIAnalysis() {
  if (isLoading.value) return
  isLoading.value = true
  aiAnalysis.value = ''

  try {
    const result = await invoke<string>('analyze_typing', {
      totalKeys: todayCount.value,
      wpm: currentWPM.value,
      apiKey: apiKey.value,
    })
    aiAnalysis.value = result
  } catch {
    aiAnalysis.value = `今天打了 ${todayCount.value} 下，Yamper 觉得你棒棒的 🐾`
  } finally {
    isLoading.value = false
  }
}

function saveApiKey() {
  localStorage.setItem('claude_api_key', apiKey.value)
}

function barHeight(keys: number): number {
  return Math.round((keys / maxKeys.value) * 60)
}

function isToday(date: string): boolean {
  return date === new Date().toISOString().slice(0, 10)
}

function shortDate(date: string): string {
  const d = new Date(date)
  return `${d.getMonth() + 1}/${d.getDate()}`
}

async function loadWeekStats() {
  weekStats.value = await invoke<{ date: string; keys: number }[]>('get_weekly_stats')
}

async function onDogMouseDown(e: MouseEvent) {
  e.preventDefault()
  await getCurrentWindow().startDragging()
}

function onMouseEnter() {
  showStats.value = true
  loadWeekStats()
}

function onMouseLeave() {
  if (!showPanel.value) showStats.value = false
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
onMounted(async () => {
  todayCount.value = await invoke<number>('get_keypress_count')

  unlisten = await listen<number>('key-press', (e) => {
    onKeyPress(e.payload)
  })

  frameTimer = setInterval(() => { frameToggle.value = !frameToggle.value }, 120)
})

onUnmounted(() => {
  unlisten?.()
  if (frameTimer) clearInterval(frameTimer)
  if (idleTimer) clearTimeout(idleTimer)
})
</script>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; }

body {
  background: transparent !important;
  overflow: hidden;
  user-select: none;
}

.app {
  width: 300px;
  height: 300px;
  position: relative;
  display: flex;
  align-items: flex-end;
  justify-content: center;
}

/* ── Dog ── */
.dog-container {
  position: relative;
  width: 200px;
  height: 200px;
  flex-shrink: 0;
}

.dog-sprite {
  width: 100%;
  height: 100%;
  image-rendering: pixelated;
  filter: drop-shadow(0 4px 8px rgba(0,0,0,0.3));
}

.dog-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 80px;
  filter: drop-shadow(0 4px 8px rgba(0,0,0,0.3));
}

.dog-container.typing .dog-sprite {
  animation: bounce 0.12s ease-in-out infinite alternate;
}

.dog-container.burst .dog-sprite {
  animation: shake 0.08s linear infinite;
  filter: drop-shadow(0 0 14px #ffdd00) drop-shadow(0 4px 8px rgba(0,0,0,0.3));
}

.dog-container.sleep .dog-sprite {
  opacity: 0.8;
  animation: breathe 3s ease-in-out infinite;
}

.zzz {
  position: absolute;
  top: -8px; right: -12px;
  color: #fff;
  font-size: 12px;
  font-weight: 700;
  font-family: system-ui;
  animation: float-up 2s ease-in-out infinite;
  text-shadow: 0 1px 4px rgba(0,0,0,0.5);
  letter-spacing: 3px;
}

.burst-effect {
  position: absolute;
  top: 0; right: 0;
  font-size: 28px;
  animation: pop 0.3s ease-out forwards;
  pointer-events: none;
}

@keyframes bounce  { from { transform: translateY(0); }    to { transform: translateY(-6px); } }
@keyframes shake   { 0%{transform:translateX(-2px) rotate(-1deg)}50%{transform:translateX(2px) rotate(1deg)}100%{transform:translateX(-2px) rotate(-1deg)} }
@keyframes breathe { 0%,100%{transform:scaleY(1)} 50%{transform:scaleY(0.97)} }
@keyframes pop     { 0%{transform:scale(0);opacity:1} 100%{transform:scale(1.5);opacity:0} }
@keyframes float-up{ 0%,100%{transform:translateY(0);opacity:1} 50%{transform:translateY(-8px);opacity:0.6} }

/* ── Stats overlay ── */
.stats-overlay {
  position: absolute;
  top: 10px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(10, 10, 20, 0.82);
  backdrop-filter: blur(10px);
  border: 1px solid rgba(255,255,255,0.1);
  border-radius: 14px;
  padding: 10px 14px;
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 148px;
}

.stat-row {
  color: #e0e0e0;
  font-size: 13px;
  font-family: 'SF Pro', system-ui, sans-serif;
  display: flex;
  justify-content: space-between;
  gap: 8px;
}

.stat-row span { color: #fff; font-weight: 600; }

.btn-row {
  display: flex;
  gap: 6px;
  margin-top: 2px;
}

.ai-btn {
  flex: 1;
  background: linear-gradient(135deg, #a78bfa, #60a5fa);
  border: none;
  border-radius: 8px;
  color: white;
  font-size: 11px;
  padding: 5px 8px;
  cursor: pointer;
  font-weight: 600;
  transition: opacity 0.15s;
  white-space: nowrap;
}

.ai-btn:hover    { opacity: 0.85; }
.ai-btn:disabled { opacity: 0.45; cursor: default; }

.icon-btn {
  background: rgba(255,255,255,0.1);
  border: 1px solid rgba(255,255,255,0.15);
  border-radius: 8px;
  padding: 5px 7px;
  cursor: pointer;
  font-size: 13px;
  transition: background 0.15s;
}

.icon-btn:hover { background: rgba(255,255,255,0.2); }

/* ── AI bubble ── */
.ai-bubble {
  position: absolute;
  top: 10px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(255,255,255,0.96);
  border-radius: 14px;
  padding: 10px 30px 10px 12px;
  max-width: 260px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.2);
}

.ai-text {
  font-size: 13px;
  color: #1a1a1a;
  line-height: 1.55;
  font-family: system-ui, sans-serif;
}

.close-btn {
  position: absolute;
  top: 5px; right: 9px;
  background: none;
  border: none;
  font-size: 18px;
  cursor: pointer;
  color: #999;
  line-height: 1;
}

/* ── Stats panel ── */
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
  border: 1px solid rgba(255,255,255,0.08);
}

.panel-close {
  position: absolute;
  top: 8px; right: 12px;
  color: #888;
}

.stats-panel h3 {
  color: #fff;
  font-size: 14px;
  font-family: system-ui;
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
  font-family: system-ui;
}

.week-total {
  color: #bbb;
  font-size: 12px;
  font-family: system-ui;
  text-align: center;
}

.api-section {
  margin-top: auto;
}

.api-input {
  width: 100%;
  background: rgba(255,255,255,0.07);
  border: 1px solid rgba(255,255,255,0.12);
  border-radius: 8px;
  padding: 6px 10px;
  color: #ccc;
  font-size: 11px;
  outline: none;
}

.api-input::placeholder { color: #555; }
.api-input:focus { border-color: rgba(167,139,250,0.5); }

/* ── Transitions ── */
.fade-enter-active, .fade-leave-active { transition: opacity 0.2s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }

.slide-up-enter-active, .slide-up-leave-active { transition: all 0.25s ease; }
.slide-up-enter-from { opacity: 0; transform: translateX(-50%) translateY(10px); }
.slide-up-leave-to   { opacity: 0; transform: translateX(-50%) translateY(10px); }
</style>
