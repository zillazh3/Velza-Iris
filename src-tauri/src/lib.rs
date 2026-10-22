pub mod anchor;
pub mod camera;
mod commands;
pub mod motion;
pub mod osc;
pub mod presets;
pub mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(commands::App::new())
        .invoke_handler(tauri::generate_handler![
            commands::connect,
            commands::status,
            commands::calibrate,
            commands::reset_defaults,
            commands::list_presets,
            commands::apply_preset,
            commands::list_motions,
            commands::play_motion,
            commands::motion_control,
            commands::motion_status,
            commands::stop_motion,
            commands::panic_stop,
            commands::start_turnaround,
            commands::turnaround_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
