use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use asusdisplay::{Feature, Kind, Monitor, Reading};
use serde::Serialize;
use tauri::State;

/// Behind one lock because a monitor answers one DDC/CI request at a time.
#[derive(Default)]
struct Monitors(Mutex<Vec<Monitor>>);

impl Monitors {
    fn lock(&self) -> MutexGuard<'_, Vec<Monitor>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Serialize)]
struct Setting {
    #[serde(flatten)]
    feature: Feature,
    #[serde(flatten)]
    reading: Option<Reading>,
    error: Option<String>,
}

fn setting(m: &mut Monitor, feature: Feature) -> Setting {
    let (reading, error) = match feature.kind {
        Kind::Action => (None, None),
        _ => match m.read(&feature) {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(e)),
        },
    };
    Setting { feature, reading, error }
}

#[tauri::command(async)]
fn monitors(state: State<'_, Monitors>) -> Result<Vec<String>, String> {
    let mut list = state.lock();
    *list = asusdisplay::enumerate(&Default::default());
    Ok(list.iter().map(|m| m.connector.as_ref().map_or_else(|| m.model.clone(), |c| format!("{} ({c})", m.model))).collect())
}

#[tauri::command(async)]
fn status(state: State<'_, Monitors>, index: usize) -> Result<Vec<Setting>, String> {
    let mut list = state.lock();
    let m = list.get_mut(index).ok_or("monitor not found")?;
    Ok(m.features(false).into_iter().map(|f| setting(m, f)).collect())
}

#[tauri::command(async)]
fn set(state: State<'_, Monitors>, index: usize, key: String, value: String, force: bool) -> Result<Setting, String> {
    let mut list = state.lock();
    let m = list.get_mut(index).ok_or("monitor not found")?;
    let feature = m.feature(&key).ok_or("unknown setting")?;
    m.write(&feature, &value, force)?;
    Ok(setting(m, feature))
}

/// XDG autostart entry, picked up by every desktop environment at login.
fn autostart_file() -> Result<PathBuf, String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or("HOME is not set")?;
    Ok(config.join("autostart/asusdisplay.desktop"))
}

#[tauri::command]
fn autostart() -> bool {
    autostart_file().is_ok_and(|f| f.exists())
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    let file = autostart_file()?;
    if !enabled {
        return match fs::remove_file(&file) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }

    // An AppImage runs from a temporary mount; APPIMAGE holds the file that stays put.
    let exe = std::env::var_os("APPIMAGE").map_or_else(std::env::current_exe, |p| Ok(p.into())).map_err(|e| e.to_string())?;
    let entry = format!("[Desktop Entry]\nType=Application\nName=AsusDisplay\nExec=\"{}\"\n", exe.display());
    fs::create_dir_all(file.parent().expect("has a parent")).and_then(|()| fs::write(&file, entry)).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .manage(Monitors::default())
        .invoke_handler(tauri::generate_handler![monitors, status, set, autostart, set_autostart])
        .run(tauri::generate_context!())
        .expect("failed to start the app");
}
