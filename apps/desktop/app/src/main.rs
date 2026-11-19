// A release build on Windows is a window app, with no console of its own.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    bandear_desktop::run();
}
