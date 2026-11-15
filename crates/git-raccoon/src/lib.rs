//! Git Raccoon: a fast, friendly desktop Git client.

mod app;
pub mod commands;
pub mod format;
mod highlight;
mod images;
pub mod repo_view;
mod search;
pub mod settings;
mod task;
mod theme;
mod ui;

pub use app::App;
