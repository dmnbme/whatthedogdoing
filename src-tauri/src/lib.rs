use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::thread;

use chrono::Local;
use rusqlite::{params, Connection};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(desktop)]
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

static KEYPRESS_COUNT: AtomicU64 = AtomicU64::new(0);

struct Db(Mutex<Connection>);

#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
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
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ]
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

    format!(
        "今天打了 {} 下，{} WPM，人设认定：{} ⚡",
        total_keys, wpm, label
    )
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "windows")]
fn windows_key_to_name(key: rdev::Key) -> &'static str {
    use rdev::Key;

    match key {
        Key::KeyA => "KeyA",
        Key::KeyB => "KeyB",
        Key::KeyC => "KeyC",
        Key::KeyD => "KeyD",
        Key::KeyE => "KeyE",
        Key::KeyF => "KeyF",
        Key::KeyG => "KeyG",
        Key::KeyH => "KeyH",
        Key::KeyI => "KeyI",
        Key::KeyJ => "KeyJ",
        Key::KeyK => "KeyK",
        Key::KeyL => "KeyL",
        Key::KeyM => "KeyM",
        Key::KeyN => "KeyN",
        Key::KeyO => "KeyO",
        Key::KeyP => "KeyP",
        Key::KeyQ => "KeyQ",
        Key::KeyR => "KeyR",
        Key::KeyS => "KeyS",
        Key::KeyT => "KeyT",
        Key::KeyU => "KeyU",
        Key::KeyV => "KeyV",
        Key::KeyW => "KeyW",
        Key::KeyX => "KeyX",
        Key::KeyY => "KeyY",
        Key::KeyZ => "KeyZ",

        Key::Num1 => "Digit1",
        Key::Num2 => "Digit2",
        Key::Num3 => "Digit3",
        Key::Num4 => "Digit4",
        Key::Num5 => "Digit5",
        Key::Num6 => "Digit6",
        Key::Num7 => "Digit7",
        Key::Num8 => "Digit8",
        Key::Num9 => "Digit9",
        Key::Num0 => "Digit0",

        Key::Space => "Space",
        Key::Return => "Enter",
        Key::Tab => "Tab",
        Key::Escape => "Escape",
        Key::Backspace => "Backspace",
        Key::ShiftLeft => "ShiftLeft",
        Key::ShiftRight => "ShiftRight",
        Key::ControlLeft => "ControlLeft",
        Key::ControlRight => "ControlRight",
        Key::Alt => "AltLeft",
        Key::AltGr => "AltRight",
        Key::MetaLeft => "MetaLeft",
        Key::MetaRight => "MetaRight",
        Key::LeftArrow => "ArrowLeft",
        Key::RightArrow => "ArrowRight",
        Key::UpArrow => "ArrowUp",
        Key::DownArrow => "ArrowDown",

        Key::Minus => "Minus",
        Key::Equal => "Equal",
        Key::LeftBracket => "BracketLeft",
        Key::RightBracket => "BracketRight",
        Key::SemiColon => "Semicolon",
        Key::Quote => "Quote",
        Key::BackSlash => "Backslash",
        Key::Comma => "Comma",
        Key::Dot => "Period",
        Key::Slash => "Slash",
        Key::BackQuote => "Backquote",

        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",

        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::Delete => "Delete",
        Key::Insert => "Insert",

        _ => "Unknown",
    }
}

#[cfg(target_os = "macos")]
fn start_keyboard_listener(app: AppHandle, db_path: std::path::PathBuf) {
    use std::collections::HashSet;
    use std::sync::Arc;

    thread::spawn(move || {
        use block2::RcBlock;
        use objc2_app_kit::{NSEvent, NSEventType};
        use objc2_foundation::NSRunLoop;

        let conn = Arc::new(Mutex::new(
            Connection::open(&db_path).expect("listener db open failed"),
        ));
        let conn_b = conn.clone();

        let flush_counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let flush_b = flush_counter.clone();

        let pressed_mods: Arc<Mutex<HashSet<u16>>> = Arc::new(Mutex::new(HashSet::new()));
        let pressed_b = pressed_mods.clone();

        let block = RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| {
            let event: &NSEvent = unsafe { event.as_ref() };
            let key_code = unsafe { event.keyCode() };

            let is_press = unsafe {
                match event.r#type() {
                    NSEventType::KeyDown => true,
                    NSEventType::FlagsChanged => {
                        let mut pressed = pressed_b.lock().unwrap();
                        if pressed.contains(&key_code) {
                            pressed.remove(&key_code);
                            false
                        } else {
                            pressed.insert(key_code);
                            true
                        }
                    }
                    _ => false,
                }
            };

            if !is_press {
                return;
            }

            let key_name = macos_keycode_to_name(key_code).to_string();
            let count = KEYPRESS_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = app.emit("key-press", KeyPressPayload { count, key: key_name });

            let n = flush_b.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 20 == 0 {
                save_today_count(&conn_b.lock().unwrap(), count);
            }
        });

        let mask: u64 = (1u64 << 10) | (1u64 << 12);
        let _monitor: Option<objc2::rc::Retained<objc2::runtime::AnyObject>> = unsafe {
            objc2::msg_send_id![
                objc2::class!(NSEvent),
                addGlobalMonitorForEventsMatchingMask: mask,
                handler: &*block
            ]
        };

        unsafe { objc2_foundation::NSRunLoop::currentRunLoop().run() };
    });
}

#[cfg(target_os = "windows")]
fn start_keyboard_listener(app: AppHandle, db_path: std::path::PathBuf) {
    use std::sync::Arc;

    thread::spawn(move || {
        use rdev::{listen, Event, EventType};

        let conn = Arc::new(Mutex::new(
            Connection::open(&db_path).expect("listener db open failed"),
        ));
        let conn_b = conn.clone();

        let flush_counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let flush_b = flush_counter.clone();

        let callback = move |event: Event| {
            let key_name = match event.event_type {
                EventType::KeyPress(key) => windows_key_to_name(key),
                _ => return,
            };

            let count = KEYPRESS_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = app.emit(
                "key-press",
                KeyPressPayload {
                    count,
                    key: key_name.to_string(),
                },
            );

            let n = flush_b.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 20 == 0 {
                save_today_count(&conn_b.lock().unwrap(), count);
            }
        };

        if let Err(err) = listen(callback) {
            eprintln!("windows keyboard listener failed: {:?}", err);
        }
    });
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn start_keyboard_listener(_app: AppHandle, _db_path: std::path::PathBuf) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
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
                                    if event.state() == ShortcutState::Pressed {
                                        let _ = app_handle.emit("toggle-pass-through", ());
                                    }
                                }
                            })
                            .build(),
                    )
                    .map_err(|e| e.to_string())?;

                app.global_shortcut()
                    .register(shortcut)
                    .map_err(|e| e.to_string())?;

                let debug_border_item = MenuItem::with_id(
                    app,
                    "debug_border",
                    "🔲 显示窗口边框",
                    true,
                    None::<&str>,
                )?;
                let ai_item = MenuItem::with_id(
                    app,
                    "ai",
                    "✨ what the dog doin?",
                    true,
                    None::<&str>,
                )?;
                let stats_item = MenuItem::with_id(
                    app,
                    "stats",
                    "📊 详情统计",
                    true,
                    None::<&str>,
                )?;
                let passthrough_item = MenuItem::with_id(
                    app,
                    "passthrough",
                    "🫥 切换穿透",
                    true,
                    None::<&str>,
                )?;
                let sep = PredefinedMenuItem::separator(app)?;
                let quit_item =
                    MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

                let sep2 = PredefinedMenuItem::separator(app)?;
                let menu = Menu::with_items(
                    app,
                    &[
                        &ai_item,
                        &stats_item,
                        &passthrough_item,
                        &sep,
                        &debug_border_item,
                        &sep2,
                        &quit_item,
                    ],
                )?;

                TrayIconBuilder::new()
                    .icon(app.default_window_icon().unwrap().clone())
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "quit" => app.exit(0),
                        "ai" => {
                            let _ = app.emit("tray-ai-analysis", ());
                        }
                        "stats" => {
                            let _ = app.emit("tray-open-stats", ());
                        }
                        "passthrough" => {
                            let _ = app.emit("toggle-pass-through", ());
                        }
                        "debug_border" => {
                            let _ = app.emit("toggle-debug-border", ());
                        }
                        _ => {}
                    })
                    .build(app)?;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}