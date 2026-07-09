pub mod backend;
mod commands;
pub mod model;
pub mod system;

use tauri_specta::{collect_commands, Builder};

fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![commands::app_version])
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
    #[test]
    fn bindings_export_types_ok() {
        super::specta_builder()
            .export(
                specta_typescript::Typescript::default(),
                std::env::temp_dir().join("ubercron-bindings-test.ts"),
            )
            .expect("types incompatibles avec specta");
    }
}
