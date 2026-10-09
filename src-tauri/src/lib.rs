mod audio;

use std::sync::mpsc::Sender;
use std::sync::Mutex;

use audio::AudioCommand;

struct AppState {
    tx: Mutex<Sender<AudioCommand>>,
}

#[tauri::command]
fn start_bell(state: tauri::State<AppState>) {
    let _ = state.tx.lock().unwrap().send(AudioCommand::Start);
}

#[tauri::command]
fn stop_bell(state: tauri::State<AppState>) {
    let _ = state.tx.lock().unwrap().send(AudioCommand::Stop);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let tx = audio::spawn_audio_thread();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState { tx: Mutex::new(tx) })
        .invoke_handler(tauri::generate_handler![start_bell, stop_bell])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
