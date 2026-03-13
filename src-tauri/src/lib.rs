use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;

use chrono::Local;
use rusqlite::{params, Connection};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

static KEYPRESS_COUNT: AtomicU64 = AtomicU64::new(0);

struct Db(Mutex<Connection>);

#[derive(Clone, serde::Serialize)]
struct KeyPressPayload {
    count: u64,
    key: String,
}

// ── DB init ────────────────────────────────────────────────────────────────

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

// ── Tauri commands ─────────────────────────────────────────────────────────

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

// ── Keyboard listener ──────────────────────────────────────────────────────

fn start_keyboard_listener(app: AppHandle, db_path: std::path::PathBuf) {
    thread::spawn(move || {
        use rdev::{listen, Event, EventType};

        let conn = Connection::open(&db_path).expect("listener db open failed");
        let mut flush_counter = 0u32;

        let callback = move |event: Event| {
            if let EventType::KeyPress(key) = event.event_type {
                let count = KEYPRESS_COUNT.fetch_add(1, Ordering::Relaxed) + 1;

                let payload = KeyPressPayload {
                    count,
                    key: format!("{:?}", key),
                };

                let _ = app.emit("key-press", payload);

                flush_counter += 1;
                if flush_counter >= 20 {
                    flush_counter = 0;
                    save_today_count(&conn, count);
                }
            }
        };

        if let Err(e) = listen(callback) {
            eprintln!("rdev error: {:?}", e);
        }
    });
}

// ── App entry ──────────────────────────────────────────────────────────────

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
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}