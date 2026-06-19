use gaming_presentation::register_commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    let builder = register_commands(builder);
    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
