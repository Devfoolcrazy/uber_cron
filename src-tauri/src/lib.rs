pub mod backend;
mod commands;
pub mod model;
pub mod system;

use std::sync::Arc;

use tauri_specta::{collect_commands, Builder};

use backend::cron::backend::CronBackend;
use commands::AppState;
use system::RealSystem;

fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::app_version,
        commands::list_jobs,
        commands::get_job,
        commands::create_job,
        commands::update_job,
        commands::delete_job,
        commands::set_job_enabled,
        commands::run_job,
        commands::run_diagnostics,
        commands::preview_schedule,
    ])
}

fn app_state() -> AppState {
    let backups_dir = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("UberCron")
        .join("backups");
    AppState {
        cron: Arc::new(CronBackend::new(Arc::new(RealSystem), backups_dir)),
    }
}

/// Exporte les bindings TS. Appelé au lancement en debug (§7 : jamais de miroir manuel).
#[cfg(debug_assertions)]
fn export_bindings(builder: &Builder<tauri::Wry>) {
    builder
        .export(
            specta_typescript::Typescript::default()
                .header("// Fichier GÉNÉRÉ par tauri-specta — ne jamais éditer à la main.\n"),
            "../src/lib/bindings.ts",
        )
        .expect("échec de l'export des bindings TypeScript");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    export_bindings(&builder);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    /// Les bindings doivent être exportables (types tous compatibles specta).
    /// En passant, on régénère src/lib/bindings.ts — même sortie que l'export
    /// au lancement en debug, donc toujours à jour après `cargo test`.
    #[test]
    fn bindings_export_types_ok() {
        super::export_bindings(&super::specta_builder());
    }
}
