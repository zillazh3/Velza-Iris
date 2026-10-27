//! Vigila el `output_log` de VRChat para saber cuándo entra un jugador (VRChat no lo avisa por OSC).
//! Línea real: `2026.10.26 20:47:19 Debug      -  [Behaviour] OnPlayerJoined Nombre (usr_...)`.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Al entrar a una instancia llegan varios joins seguidos: se avisa una vez por ventana.
const DEBOUNCE: Duration = Duration::from_secs(2);

pub fn is_player_joined(line: &str) -> bool {
    line.contains("[Behaviour] OnPlayerJoined ")
}

fn log_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")?;
    Some(PathBuf::from(home).join("AppData/LocalLow/VRChat/VRChat"))
}

/// El log de la sesión más reciente (VRChat crea un `output_log_*.txt` por sesión).
fn latest_log(dir: &PathBuf) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("output_log_"))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

/// Hilo que llama a `on_join` cuando aparece un `OnPlayerJoined` nuevo. Abre el log al final: no reproduce joins viejos.
pub fn watch(on_join: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let Some(dir) = log_dir() else { return };
        let mut open: Option<(PathBuf, BufReader<File>)> = None;
        let mut last_notify: Option<Instant> = None;
        // Persistente: una línea a medio escribir se completa en el siguiente sondeo.
        let mut line = String::new();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let Some(latest) = latest_log(&dir) else { continue };
            if open.as_ref().map(|(p, _)| p) != Some(&latest) {
                let Ok(mut f) = File::open(&latest) else { continue };
                let _ = f.seek(SeekFrom::End(0));
                open = Some((latest, BufReader::new(f)));
                line.clear();
            }
            let Some((_, reader)) = open.as_mut() else { continue };
            let mut joined = false;
            // `read_line` devuelve 0 en el final actual del archivo.
            while reader.read_line(&mut line).is_ok_and(|n| n > 0) {
                if line.ends_with('\n') {
                    joined |= is_player_joined(&line);
                    line.clear();
                } else {
                    break;
                }
            }
            if joined && last_notify.map_or(true, |t| t.elapsed() >= DEBOUNCE) {
                last_notify = Some(Instant::now());
                on_join();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::is_player_joined;

    #[test]
    fn matches_only_the_behaviour_join_line() {
        assert!(is_player_joined("2026.10.26 20:47:19 Debug      -  [Behaviour] OnPlayerJoined Zilla-ZH3 (usr_abc)"));
        assert!(!is_player_joined("2026.10.26 20:47:54 Debug      -  [Behaviour] OnPlayerLeft Zilla-ZH3 (usr_abc)"));
        assert!(!is_player_joined("2026.10.26 20:47:19 Debug      -  [VisitorsInformationBoard] 12.94 / OnPlayerJoined / player=Zilla-ZH3(local)"));
    }
}
