//! Aeromessage - Tauri app entry point.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use aeromessage::{Database, Conversation, ContactResolver, send_message, mark_as_read};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::process::Command;
use tauri::State;

/// Application state shared across commands.
struct AppState {
    drafts: Mutex<HashMap<i64, String>>,
    committed: Mutex<HashMap<i64, String>>,
    later: Mutex<HashSet<i64>>,
    ignored: Mutex<HashSet<String>>,
    contacts: Mutex<ContactResolver>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            drafts: Mutex::new(HashMap::new()),
            committed: Mutex::new(HashMap::new()),
            later: Mutex::new(HashSet::new()),
            ignored: Mutex::new(HashSet::new()),
            contacts: Mutex::new(ContactResolver::new()),
        }
    }
}

#[tauri::command]
fn get_conversations(state: State<AppState>) -> Result<Vec<Conversation>, String> {
    let path = Database::default_path();
    let db = Database::open(&path).map_err(|e| e.to_string())?;
    let mut convs = db.unread_conversations().map_err(|e| e.to_string())?;
    
    // Resolve contact names
    let contacts = state.contacts.lock().map_err(|e| e.to_string())?;
    for conv in &mut convs {
        if conv.display_name.is_none() || conv.display_name.as_ref().map(|s| s.is_empty()).unwrap_or(false) {
            if conv.is_group() {
                // For groups, resolve participant names
                let names: Vec<String> = conv.participants.iter()
                    .filter_map(|p| contacts.resolve(p).map(|n| {
                        // Use first name only for groups
                        n.split_whitespace().next().unwrap_or(n).to_string()
                    }))
                    .collect();
                
                if !names.is_empty() {
                    conv.resolved_name = Some(names.join(", "));
                }
            } else {
                // For 1:1 chats, resolve the identifier
                if let Some(name) = contacts.resolve(&conv.chat_identifier) {
                    conv.resolved_name = Some(name.to_string());
                }
            }
        }
    }
    
    Ok(convs)
}

#[tauri::command]
fn save_draft(chat_id: i64, text: String, state: State<AppState>) -> Result<String, String> {
    let mut drafts = state.drafts.lock().map_err(|e| e.to_string())?;
    let mut committed = state.committed.lock().map_err(|e| e.to_string())?;
    
    // Remove from committed if editing
    committed.remove(&chat_id);
    
    let result = if text.trim().is_empty() {
        drafts.remove(&chat_id);
        "empty"
    } else {
        drafts.insert(chat_id, text);
        "draft"
    };
    
    Ok(result.to_string())
}

#[tauri::command]
fn commit_message(chat_id: i64, text: String, state: State<AppState>) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("No text provided".to_string());
    }
    
    let mut drafts = state.drafts.lock().map_err(|e| e.to_string())?;
    let mut committed = state.committed.lock().map_err(|e| e.to_string())?;
    
    drafts.remove(&chat_id);
    committed.insert(chat_id, text);
    
    Ok("committed".to_string())
}

#[tauri::command]
fn toggle_later(chat_id: i64, state: State<AppState>) -> Result<bool, String> {
    let mut later = state.later.lock().map_err(|e| e.to_string())?;
    let mut drafts = state.drafts.lock().map_err(|e| e.to_string())?;
    let mut committed = state.committed.lock().map_err(|e| e.to_string())?;
    
    let is_later = if later.contains(&chat_id) {
        later.remove(&chat_id);
        false
    } else {
        later.insert(chat_id);
        drafts.remove(&chat_id);
        committed.remove(&chat_id);
        true
    };
    
    Ok(is_later)
}

#[tauri::command]
fn toggle_ignore(chat_identifier: String, state: State<AppState>) -> Result<bool, String> {
    let mut ignored = state.ignored.lock().map_err(|e| e.to_string())?;
    
    let is_ignored = if ignored.contains(&chat_identifier) {
        ignored.remove(&chat_identifier);
        false
    } else {
        ignored.insert(chat_identifier);
        true
    };
    
    Ok(is_ignored)
}

#[tauri::command]
fn send_all(state: State<AppState>) -> Result<Vec<SendResult>, String> {
    let path = Database::default_path();
    let db = Database::open(&path).map_err(|e| e.to_string())?;
    let convs = db.unread_conversations().map_err(|e| e.to_string())?;
    
    let conv_map: HashMap<i64, &Conversation> = convs.iter()
        .map(|c| (c.chat_id, c))
        .collect();
    
    let mut committed = state.committed.lock().map_err(|e| e.to_string())?;
    let to_send: Vec<_> = committed.drain().collect();
    
    let mut results = Vec::new();
    for (chat_id, text) in to_send {
        if let Some(conv) = conv_map.get(&chat_id) {
            let success = send_message(&conv.chat_identifier, &text, conv.is_group()).is_ok();
            if success {
                // Mark conversation as read after successful send
                let _ = mark_as_read(&conv.chat_identifier);
            }
            results.push(SendResult {
                chat_id,
                success,
                name: conv.name().to_string(),
            });
        }
    }
    
    Ok(results)
}

#[tauri::command]
fn mark_read(chat_identifier: String) -> Result<usize, String> {
    mark_as_read(&chat_identifier).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
struct SendResult {
    chat_id: i64,
    success: bool,
    name: String,
}

#[tauri::command]
fn get_state(state: State<AppState>) -> Result<StateSnapshot, String> {
    let drafts = state.drafts.lock().map_err(|e| e.to_string())?;
    let committed = state.committed.lock().map_err(|e| e.to_string())?;
    let later = state.later.lock().map_err(|e| e.to_string())?;
    let ignored = state.ignored.lock().map_err(|e| e.to_string())?;
    
    Ok(StateSnapshot {
        drafts: drafts.clone(),
        committed: committed.clone(),
        later: later.iter().cloned().collect(),
        ignored: ignored.iter().cloned().collect(),
    })
}

#[derive(serde::Serialize)]
struct StateSnapshot {
    drafts: HashMap<i64, String>,
    committed: HashMap<i64, String>,
    later: Vec<i64>,
    ignored: Vec<String>,
}

#[tauri::command]
fn open_full_disk_access() -> Result<(), String> {
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn load_contacts(state: State<AppState>) -> Result<usize, String> {
    let mut contacts = state.contacts.lock().map_err(|e| e.to_string())?;
    contacts.load_macos_contacts()
}

#[tauri::command]
fn get_attachment(path: String) -> Result<Vec<u8>, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let attachments_dir = home.join("Library/Messages/Attachments");
    let full_path = attachments_dir.join(&path);
    
    // Resolve to prevent path traversal
    let canonical = full_path.canonicalize().map_err(|e| e.to_string())?;
    let canonical_base = attachments_dir.canonicalize().map_err(|e| e.to_string())?;
    
    if !canonical.starts_with(&canonical_base) {
        return Err("Access denied".to_string());
    }
    
    // Read the file
    let data = std::fs::read(&canonical).map_err(|e| e.to_string())?;
    
    // Convert HEIC to JPEG if needed
    let extension = canonical.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    if extension == "heic" || extension == "heif" {
        let cache_dir = home.join("Library/Caches/Aeromessage");
        std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
        
        let cache_key = path.replace('/', "_").replace('.', "_") + ".jpg";
        let cached_path = cache_dir.join(&cache_key);
        
        if !cached_path.exists() {
            Command::new("sips")
                .args(["-s", "format", "jpeg", "-s", "formatOptions", "80"])
                .arg(&canonical)
                .arg("--out")
                .arg(&cached_path)
                .output()
                .map_err(|e| e.to_string())?;
        }
        
        if cached_path.exists() {
            return std::fs::read(&cached_path).map_err(|e| e.to_string());
        }
    }
    
    Ok(data)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_conversations,
            save_draft,
            commit_message,
            toggle_later,
            toggle_ignore,
            send_all,
            mark_read,
            get_state,
            get_version,
            open_full_disk_access,
            open_url,
            load_contacts,
            get_attachment,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
