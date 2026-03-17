// Prevents an extra console window when launching the GUI app on Windows.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    switchfetcher_lib::run()
}
