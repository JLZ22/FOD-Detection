// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if !cfg!(debug_assertions) {
        std::env::set_current_dir(std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("Resources")).unwrap();
    }
    app::run();
}
