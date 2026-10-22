//! Carpeta portable `VelzaIris-data/` junto al .exe (o en %LOCALAPPDATA% si el usuario lo elige).
use std::path::PathBuf;

pub fn data_dir(use_local_appdata: bool) -> Result<PathBuf, String> {
    let base = if use_local_appdata {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("no existe %LOCALAPPDATA%")?)
    } else {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        exe.parent().ok_or("el .exe no tiene carpeta")?.to_path_buf()
    };
    let dir = base.join("VelzaIris-data");
    let probe = dir.join(".write-test");
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&probe, b""))
        .and_then(|_| std::fs::remove_file(&probe))
        .map_err(|e| format!("{} no es escribible ({e}); puedes elegir %LOCALAPPDATA% en Ajustes", dir.display()))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    #[test]
    fn creates_portable_dir_next_to_exe() {
        let dir = super::data_dir(false).unwrap();
        assert!(dir.ends_with("VelzaIris-data") && dir.is_dir());
        assert!(dir.parent().unwrap() == std::env::current_exe().unwrap().parent().unwrap());
    }
}
