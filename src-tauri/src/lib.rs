mod commands;
mod db;
mod meta;

use commands::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // WebKitGTK's DMA-BUF renderer segfaults on some Linux GPU stacks (AMD/Wayland incl.)
    #[cfg(target_os = "linux")]
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let meta_pool =
                tauri::async_runtime::block_on(meta::init(&data_dir.join("meta.db")))?;
            app.manage(AppState {
                meta: meta_pool,
                pools: Default::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::test_connection,
            commands::connect,
            commands::disconnect,
            commands::list_tables,
            commands::table_schema,
            commands::fetch_rows,
            commands::run_query,
            commands::update_cell,
            commands::insert_row,
            commands::delete_row,
            commands::add_column,
            commands::list_saved_queries,
            commands::save_query,
            commands::delete_saved_query,
            commands::export_rows,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
