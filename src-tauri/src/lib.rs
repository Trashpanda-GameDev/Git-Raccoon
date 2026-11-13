pub mod details;
pub mod graph;
pub mod refs;
pub mod repo;
#[cfg(test)]
mod testutil;

#[tauri::command]
fn load_graph(path: String, limit: Option<usize>) -> Result<repo::Graph, String> {
    repo::load_graph(&path, limit.unwrap_or(2000)).map_err(|e| e.message().to_string())
}

#[tauri::command]
fn commit_details(path: String, id: String) -> Result<details::CommitDetails, String> {
    details::commit_details(&path, &id).map_err(|e| e.message().to_string())
}

#[tauri::command]
fn file_diff(path: String, id: String, file: String) -> Result<details::FileDiff, String> {
    details::file_diff(&path, &id, &file).map_err(|e| e.message().to_string())
}

#[tauri::command]
fn load_refs(path: String) -> Result<refs::Refs, String> {
    refs::load_refs(&path).map_err(|e| e.message().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            load_graph,
            load_refs,
            commit_details,
            file_diff
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
