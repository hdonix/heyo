// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // How WebKitGTK is to draw: decided from the settings and the graphics
    // driver before the first web view (see renderer.rs).
    heyo_lib::renderer::apply();

    heyo_lib::run()
}
