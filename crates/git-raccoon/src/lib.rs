//! Git Raccoon: a fast, friendly desktop Git client.

mod app;
pub mod changelog;
pub mod commands;
pub mod crash;
pub mod feedback;
pub mod forge;
pub mod forge_view;
pub mod format;
mod highlight;
mod images;
pub mod repo_view;
mod search;
pub mod settings;
mod task;
pub mod terminal;
mod theme;
pub mod themes;
mod title_bar;
mod ui;
pub mod updates;

pub use app::App;
