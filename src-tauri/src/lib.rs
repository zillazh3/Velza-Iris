pub mod anchor;
pub mod camera;
mod commands;
pub mod motion;
pub mod osc;
pub mod presets;
pub mod storage;
pub mod studio;

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
            commands::save_user_preset,
            commands::import_preset_json,
            commands::list_motions,
            commands::play_motion,
            commands::motion_control,
            commands::motion_status,
            commands::stop_motion,
            commands::panic_stop,
            commands::start_turnaround,
            commands::turnaround_status,
            commands::capture_camera_point,
            commands::capture_target_point,
            commands::evaluate_sequence,
            commands::preview_trajectory,
            commands::play_sequence,
            commands::studio_control,
            commands::studio_status,
            commands::stop_studio,
            commands::start_recording,
            commands::recording_status,
            commands::stop_recording,
            commands::resimplify_recording,
            commands::list_sequences,
            commands::save_sequence_cmd,
            commands::delete_sequence_cmd,
            commands::import_sequence_cmd,
            commands::telemetry_status,
            commands::reset_calibration_cmd,
            commands::load_motion_as_sequence,
            commands::import_vrc_dolly_cmd,
            commands::export_vrc_dolly_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
