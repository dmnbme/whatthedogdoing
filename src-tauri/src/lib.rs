use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

use chrono::Local;
use rusqlite::{params, Connection};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

static KEYPRESS_COUNT: AtomicU64 = AtomicU64::new(0);

// ── Global state for CGEventTap C callback ───────────────────────────────────

static GLOBAL_APP: OnceLock<AppHandle> = OnceLock::new();
static GLOBAL_DB_CONN: OnceLock<Mutex<Connection>> = OnceLock::new();
static GLOBAL_PRESSED_MODS: OnceLock<Mutex<HashSet<u16>>> = OnceLock::new();

// ── macOS framework bindings ────────────────────────────────────────────────

#[cfg(target_os = "macos")]
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOHIDCheckAccess(request_type: u32) -> u32; // 0=granted, 1=denied, 2=unknown
    fn IOHIDRequestAccess(request_type: u32) -> bool;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: unsafe extern "C" fn(
            *mut std::ffi::c_void,
            u32,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void,
        user_info: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
    fn CGEventTapEnable(tap: *mut std::ffi::c_void, enable: bool);
    fn CGEventGetIntegerValueField(event: *mut std::ffi::c_void, field: i32) -> i64;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *const std::ffi::c_void,
        port: *mut std::ffi::c_void,
        order: isize,
    ) -> *mut std::ffi::c_void;
    fn CFRunLoopAddSource(
        rl: *mut std::ffi::c_void,
        source: *mut std::ffi::c_void,
        mode: *const std::ffi::c_void,
    );
    fn CFRunLoopGetCurrent() -> *mut std::ffi::c_void;
    fn CFRunLoopRun();
    static kCFRunLoopCommonModes: *const std::ffi::c_void;
}

// ── DB ──────────────────────────────────────────────────────────────────────

struct Db(Mutex<Connection>);

#[derive(Clone, serde::Serialize)]
struct KeyPressPayload {
    count: u64,
    key: String,
}

fn init_db(conn: &Connection) {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS daily_stats (
            date TEXT PRIMARY KEY,
            total_keys INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            key_count INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .expect("failed to init db");
}

fn load_today_count(conn: &Connection) -> u64 {
    let today = Local::now().format("%Y-%m-%d").to_string();
    conn.query_row(
        "SELECT total_keys FROM daily_stats WHERE date = ?1",
        params![today],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0) as u64
}

fn save_today_count(conn: &Connection, count: u64) {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let now = Local::now().to_rfc3339();
    let _ = conn.execute(
        "INSERT INTO daily_stats (date, total_keys, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(date) DO UPDATE SET total_keys = ?2, updated_at = ?3",
        params![today, count as i64, now],
    );
}

// ── Accessibility check ─────────────────────────────────────────────────────

/// Returns true if Input Monitoring permission is granted.
/// Automatically triggers the system prompt when status is unknown (first run).
#[tauri::command]
fn check_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        // kIOHIDRequestTypeListenEvent = 1
        let status = unsafe { IOHIDCheckAccess(1) };
        match status {
            0 => true,  // granted
            2 => {
                // unknown — show the system permission prompt
                unsafe { IOHIDRequestAccess(1) };
                false
            }
            _ => false, // denied
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
            .spawn();
    }
}

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
fn get_keypress_count() -> u64 {
    KEYPRESS_COUNT.load(Ordering::Relaxed)
}

#[tauri::command]
fn reset_keypress_count(db: State<Db>) {
    KEYPRESS_COUNT.store(0, Ordering::Relaxed);
    let conn = db.0.lock().unwrap();
    save_today_count(&conn, 0);
}

#[tauri::command]
fn get_weekly_stats(db: State<Db>) -> Vec<serde_json::Value> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT date, total_keys FROM daily_stats
             WHERE date >= date('now', '-6 days')
             ORDER BY date ASC",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "date": row.get::<_, String>(0)?,
            "keys": row.get::<_, i64>(1)?,
        }))
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

#[tauri::command]
async fn set_window_ignore_cursor(app: AppHandle, ignore: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("no main window".to_string())?;
    window
        .set_ignore_cursor_events(ignore)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn analyze_typing(total_keys: u64, wpm: u32, api_key: String) -> Result<String, String> {
    if api_key.trim().is_empty() {
        return Ok(fun_fallback(total_keys, wpm));
    }
    let client = reqwest::Client::new();
    let prompt = format!(
        "用户今天的打字数据：总击键 {} 次，当前 WPM {}。\
         用 1-2 句话、轻松幽默的口吻（中文）分析他的打字状态，\
         并给他一个有趣的「打字人设」标签，比如「凌晨码字侠」「爆发型选手」。\
         回复控制在 50 字以内。",
        total_keys, wpm
    );
    let body = serde_json::json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 150,
        "messages": [{ "role": "user", "content": prompt }]
    });
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let text = json["content"][0]["text"]
        .as_str()
        .unwrap_or("Yamper 今天也很努力 🐾")
        .to_string();
    Ok(text)
}

fn fun_fallback(total_keys: u64, wpm: u32) -> String {
    let label = if wpm > 80 {
        "「爆发型选手」"
    } else if wpm > 50 {
        "「稳健码字侠」"
    } else if total_keys > 5000 {
        "「耐力型选手」"
    } else {
        "「摸鱼预备役」"
    };
    format!("今天打了 {} 下，{} WPM，人设认定：{} ⚡", total_keys, wpm, label)
}

// ── Keycode mapping ─────────────────────────────────────────────────────────

fn macos_keycode_to_name(code: u16) -> &'static str {
    match code {
        0 => "KeyA", 1 => "KeyS", 2 => "KeyD", 3 => "KeyF", 4 => "KeyH",
        5 => "KeyG", 6 => "KeyZ", 7 => "KeyX", 8 => "KeyC", 9 => "KeyV",
        11 => "KeyB", 12 => "KeyQ", 13 => "KeyW", 14 => "KeyE", 15 => "KeyR",
        16 => "KeyY", 17 => "KeyT",
        18 => "Digit1", 19 => "Digit2", 20 => "Digit3", 21 => "Digit4",
        22 => "Digit6", 23 => "Digit5", 24 => "Equal", 25 => "Digit9",
        26 => "Digit7", 27 => "Minus", 28 => "Digit8", 29 => "Digit0",
        30 => "BracketRight", 31 => "KeyO", 32 => "KeyU", 33 => "BracketLeft",
        34 => "KeyI", 35 => "KeyP", 36 => "Return", 37 => "KeyL",
        38 => "KeyJ", 39 => "Quote", 40 => "KeyK", 41 => "Semicolon",
        42 => "Backslash", 43 => "Comma", 44 => "Slash", 45 => "KeyN",
        46 => "KeyM", 47 => "Period",
        48 => "Tab", 49 => "Space", 50 => "Backquote", 51 => "Backspace",
        53 => "Escape",
        54 => "MetaRight", 55 => "MetaLeft",
        56 => "ShiftLeft", 57 => "CapsLock", 58 => "Alt", 59 => "ControlLeft",
        60 => "ShiftRight", 61 => "AltRight", 62 => "ControlRight", 63 => "Fn",
        96 => "F5", 97 => "F6", 98 => "F7", 99 => "F3", 100 => "F8",
        101 => "F9", 103 => "F11", 109 => "F10", 111 => "F12",
        118 => "F4", 120 => "F2", 122 => "F1",
        115 => "Home", 116 => "PageUp", 117 => "Delete",
        119 => "End", 121 => "PageDown",
        123 => "ArrowLeft", 124 => "ArrowRight", 125 => "ArrowDown", 126 => "ArrowUp",
        _ => "Unknown",
    }
}

// ── CGEventTap keyboard listener ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
unsafe extern "C" fn cg_event_callback(
    _proxy: *mut std::ffi::c_void,
    event_type: u32,
    event: *mut std::ffi::c_void,
    _refcon: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    // kCGKeyboardEventKeycode field = 9
    let key_code = CGEventGetIntegerValueField(event, 9) as u16;
    let key_name = macos_keycode_to_name(key_code).to_string();

    eprintln!("[kbd] event_type={} keycode={} key={}", event_type, key_code, key_name);

    let app = match GLOBAL_APP.get() {
        Some(a) => a,
        None => return event,
    };

    match event_type {
        10 => {
            // kCGEventKeyDown — emit press; release comes from KeyUp (event 11)
            let count = KEYPRESS_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = app.emit("key-press", KeyPressPayload { count, key: key_name });
            if count % 20 == 0 {
                if let Some(conn_mutex) = GLOBAL_DB_CONN.get() {
                    if let Ok(conn) = conn_mutex.try_lock() {
                        save_today_count(&conn, count);
                    }
                }
            }
        }
        11 => {
            // kCGEventKeyUp — emit release immediately (zero delay)
            let _ = app.emit("key-release", key_name);
        }
        12 => {
            // kCGEventFlagsChanged — modifier keys
            if key_code == 57 {
                // CapsLock: macOS FlagsChanged fires on both press and release, but release
                // detection is unreliable across OS versions. Match BongoCat's approach:
                // treat every FlagsChanged as a tap — press immediately, auto-release after
                // 150 ms. The gen counter debounces rapid events (press+release pair) so
                // only the LAST timer fires, preventing a stuck "no hand" state.
                static CAPS_GEN: AtomicU64 = AtomicU64::new(0);
                let gen = CAPS_GEN.fetch_add(1, Ordering::Relaxed) + 1;
                let count = KEYPRESS_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = app.emit("key-press", KeyPressPayload { count, key: key_name.clone() });
                let app_rel = app.clone();
                thread::spawn(move || {
                    thread::sleep(std::time::Duration::from_millis(150));
                    if CAPS_GEN.load(Ordering::Relaxed) == gen {
                        let _ = app_rel.emit("key-release", key_name);
                    }
                });
            } else {
                // Other modifier keys: FlagsChanged fires on press AND release,
                // so toggle in/out of pressed set to track state.
                if let Some(pressed_mods) = GLOBAL_PRESSED_MODS.get() {
                    if let Ok(mut pressed) = pressed_mods.try_lock() {
                        if pressed.contains(&key_code) {
                            pressed.remove(&key_code);
                            let _ = app.emit("key-release", key_name);
                        } else {
                            pressed.insert(key_code);
                            let count = KEYPRESS_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
                            let _ = app.emit("key-press", KeyPressPayload { count, key: key_name });
                        }
                    }
                }
            }
        }
        _ => {}
    }

    event
}

fn start_keyboard_listener(app: AppHandle, db_path: std::path::PathBuf) {
    thread::spawn(move || {
        #[cfg(target_os = "macos")]
        {
            // Initialize globals for the C callback
            let conn = Connection::open(&db_path).expect("listener db open failed");
            let _ = GLOBAL_APP.set(app);
            let _ = GLOBAL_DB_CONN.set(Mutex::new(conn));
            let _ = GLOBAL_PRESSED_MODS.set(Mutex::new(HashSet::new()));

            // kCGEventKeyDown=10, kCGEventKeyUp=11, kCGEventFlagsChanged=12
            let mask: u64 = (1u64 << 10) | (1u64 << 11) | (1u64 << 12);

            let tap = unsafe {
                CGEventTapCreate(
                    1, // kCGSessionEventTap — needs Input Monitoring (grantable to CLI binaries)
                    0, // kCGHeadInsertEventTap
                    1, // kCGEventTapOptionListenOnly (passive, doesn't block events)
                    mask,
                    cg_event_callback,
                    std::ptr::null_mut(),
                )
            };

            eprintln!("[kbd] CGEventTap created: {}", !tap.is_null());

            if tap.is_null() {
                eprintln!("[kbd] Failed to create CGEventTap — check Accessibility/Input Monitoring permission");
                return;
            }

            unsafe {
                let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
                CGEventTapEnable(tap, true);
                let rl = CFRunLoopGetCurrent();
                CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
                CFRunLoopRun();
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (app, db_path);
        }
    });
}

// ── App entry ───────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            check_accessibility,
            open_accessibility_settings,
            get_keypress_count,
            reset_keypress_count,
            get_weekly_stats,
            set_window_ignore_cursor,
            analyze_typing,
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir().unwrap();
            let _ = std::fs::create_dir_all(&data_dir);
            let db_path = data_dir.join("stats.db");

            let conn = Connection::open(&db_path).expect("db open failed");
            init_db(&conn);

            let saved = load_today_count(&conn);
            KEYPRESS_COUNT.store(saved, Ordering::Relaxed);

            app.manage(Db(Mutex::new(conn)));
            start_keyboard_listener(app.handle().clone(), db_path);

            #[cfg(desktop)]
            {
                let shortcut =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD);

                let app_handle = app.handle().clone();
                let shortcut_for_handler = shortcut.clone();

                app.handle()
                    .plugin(
                        tauri_plugin_global_shortcut::Builder::new()
                            .with_handler(move |_app, triggered_shortcut, event| {
                                if triggered_shortcut == &shortcut_for_handler {
                                    match event.state() {
                                        ShortcutState::Pressed => {
                                            let _ = app_handle.emit("toggle-pass-through", ());
                                        }
                                        ShortcutState::Released => {}
                                    }
                                }
                            })
                            .build(),
                    )
                    .map_err(|e| e.to_string())?;

                app.global_shortcut()
                    .register(shortcut)
                    .map_err(|e| e.to_string())?;

                let debug_border_item = MenuItem::with_id(app, "debug_border", "🔲 显示窗口边框", true, None::<&str>)?;
                let ai_item = MenuItem::with_id(app, "ai", "✨ what the dog doin?", true, None::<&str>)?;
                let stats_item = MenuItem::with_id(app, "stats", "📊 详情统计", true, None::<&str>)?;
                let passthrough_item = MenuItem::with_id(app, "passthrough", "🫥 切换穿透", true, None::<&str>)?;
                let sep = PredefinedMenuItem::separator(app)?;
                let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

                let sep2 = PredefinedMenuItem::separator(app)?;
                let menu = Menu::with_items(app, &[&ai_item, &stats_item, &passthrough_item, &sep, &debug_border_item, &sep2, &quit_item])?;

                TrayIconBuilder::new()
                    .icon(app.default_window_icon().unwrap().clone())
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "quit" => app.exit(0),
                        "ai" => { let _ = app.emit("tray-ai-analysis", ()); }
                        "stats" => { let _ = app.emit("tray-open-stats", ()); }
                        "passthrough" => { let _ = app.emit("toggle-pass-through", ()); }
                        "debug_border" => { let _ = app.emit("toggle-debug-border", ()); }
                        _ => {}
                    })
                    .build(app)?;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
