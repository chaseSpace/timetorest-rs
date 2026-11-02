#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod runtime;
#[cfg(windows)]
mod windows;

fn main() {
    #[cfg(windows)]
    if let Err(error) = windows::run() {
        windows::report_error(&error);
    }
    #[cfg(not(windows))]
    eprintln!("This development build currently implements the Windows native UI only.");
}
