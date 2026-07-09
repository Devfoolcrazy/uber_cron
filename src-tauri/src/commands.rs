//! Commands Tauri : couche fine, aucune logique métier (§7).

#[tauri::command]
#[specta::specta]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
