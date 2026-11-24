//! Git Raccoon: a fast, friendly desktop Git client.

pub mod agent;
mod app;
pub mod bisect_view;
pub mod changelog;
pub mod commands;
pub mod crash;
mod emoji;
pub mod feedback;
pub mod forge;
pub mod forge_view;
pub mod format;
mod highlight;
mod images;
pub mod profiles;
pub mod repo_view;
mod search;
pub mod settings;
mod task;
pub mod terminal;
mod theme;
pub mod themes;
mod title_bar;
pub mod tools;
mod ui;
pub mod updates;

pub use app::App;
