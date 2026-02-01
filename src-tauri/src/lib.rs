mod app;
mod command;
mod config;
mod fs;
mod log;
mod model;
mod net;
mod operation;
mod probe;
mod resources;
mod service;
mod socket;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::run();
}
