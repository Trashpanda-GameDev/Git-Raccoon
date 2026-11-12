pub mod graph;
pub mod repo;

#[tauri::command]
fn load_graph(path: String, limit: Option<usize>) -> Result<repo::Graph, String> {
    repo::load_graph(&path, limit.unwrap_or(2000)).map_err(|e| e.message().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![load_graph])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
