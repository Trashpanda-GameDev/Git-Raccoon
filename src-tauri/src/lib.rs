pub mod details;
pub mod graph;
pub mod refs;
pub mod repo;
#[cfg(test)]
mod testutil;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use repo::{GraphPage, History};

/// The history of the open repository, shared by the paging commands.
#[derive(Default)]
struct AppState {
    history: Mutex<Option<Arc<History>>>,
    next_snapshot: AtomicU64,
}

impl AppState {
    fn history(&self, snapshot: u64) -> Result<Arc<History>, String> {
        match self.history.lock().unwrap().as_ref() {
            Some(h) if h.snapshot == snapshot => Ok(h.clone()),
            _ => Err(STALE.into()),
        }
    }
}

/// Returned when a page is requested from a history that has been reloaded;
/// the UI reloads from the start when it sees this.
const STALE: &str = "stale snapshot";

/// Runs Git work on a blocking thread so the window stays responsive.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, git2::Error> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.message().to_string())
}

/// Loads a repository's history from scratch and returns its first rows.
#[tauri::command]
async fn load_graph(
    state: tauri::State<'_, AppState>,
    path: String,
    limit: usize,
) -> Result<GraphPage, String> {
    let snapshot = state.next_snapshot.fetch_add(1, Ordering::SeqCst) + 1;
    let history = Arc::new(blocking(move || History::load(&path, snapshot)).await?);
    let page = {
        let h = history.clone();
        blocking(move || h.page(0, limit)).await?
    };
    // Loads can finish out of order; never replace a newer history.
    let mut current = state.history.lock().unwrap();
    if current.as_ref().is_none_or(|h| h.snapshot < snapshot) {
        *current = Some(history);
    }
    Ok(page)
}

#[tauri::command]
async fn graph_page(
    state: tauri::State<'_, AppState>,
    snapshot: u64,
    offset: usize,
    limit: usize,
) -> Result<GraphPage, String> {
    let history = state.history(snapshot)?;
    blocking(move || history.page(offset, limit)).await
}

/// The row of a commit in the loaded history, or `None` if it isn't in it.
#[tauri::command]
fn locate_commit(
    state: tauri::State<'_, AppState>,
    snapshot: u64,
    id: String,
) -> Result<Option<usize>, String> {
    Ok(state.history(snapshot)?.locate(&id))
}

#[tauri::command]
async fn load_refs(path: String) -> Result<refs::Refs, String> {
    blocking(move || refs::load_refs(&path)).await
}

#[tauri::command]
async fn commit_details(path: String, id: String) -> Result<details::CommitDetails, String> {
    blocking(move || details::commit_details(&path, &id)).await
}

#[tauri::command]
async fn file_diff(path: String, id: String, file: String) -> Result<details::FileDiff, String> {
    blocking(move || details::file_diff(&path, &id, &file)).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_graph,
            graph_page,
            locate_commit,
            load_refs,
            commit_details,
            file_diff
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
