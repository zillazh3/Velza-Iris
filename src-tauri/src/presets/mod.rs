//! Presets de encuadre + look: JSON versionado, validado al importar, y su traducción a comandos OSC.
use crate::anchor::{Anchor, Frame};
use crate::camera::Camera;
use crate::osc::protocol::{self, Pose, Value};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::path::Path;

const SCHEMA_VERSION: u32 = 1;
const SEEDS: &str = include_str!("seeds.json");

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Retrato,
    Cuerpo,
    Ambiente,
    Movimiento,
    Video,
    RefSheet,
}

/// Modo de enfoque requerido. VRChat no expone un endpoint para cambiarlo: es solo una pista para la UI
/// (Aperture necesita Semi/Manual y FocalDistance necesita Manual).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Focus {
    Auto,
    Semi,
    Manual,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Look {
    pub zoom: Option<f32>,
    pub exposure: Option<f32>,
    pub focus: Option<Focus>,
    pub focal_distance: Option<f32>,
    pub aperture: Option<f32>,
    pub auto_level_pitch: Option<bool>,
    pub auto_level_roll: Option<bool>,
    pub look_at_me: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub schema_version: u32,
    pub name: String,
    pub category: Category,
    /// Modificador apilable: su `exposure` se suma a la actual en vez de reemplazarla.
    #[serde(default)]
    pub modifier: bool,
    pub frame: Option<Frame>,
    #[serde(default)]
    pub look: Look,
}

/// Claves de `input` que `known` (el preset re-serializado, con todas las claves) no tiene.
fn extra_keys(input: &Json, known: &Json, path: &str, out: &mut Vec<String>) {
    let (Some(i), Some(k)) = (input.as_object(), known.as_object()) else { return };
    for (key, v) in i {
        let p = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
        match k.get(key) {
            None => out.push(p),
            Some(kv) => extra_keys(v, kv, &p, out),
        }
    }
}

fn check(what: &str, v: f32, lo: f32, hi: f32) -> Result<(), String> {
    if v.is_finite() && (lo..=hi).contains(&v) { Ok(()) } else { Err(format!("{what} fuera de rango: {v}")) }
}

impl Preset {
    /// Importa un preset: rechaza versión/valores inválidos; las claves desconocidas se ignoran y se devuelven como avisos.
    pub fn from_json(s: &str) -> Result<(Preset, Vec<String>), String> {
        let raw: Json = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let p: Preset = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        p.validate()?;
        let mut warnings = Vec::new();
        extra_keys(&raw, &serde_json::to_value(&p).map_err(|e| e.to_string())?, "", &mut warnings);
        Ok((p, warnings.into_iter().map(|k| format!("clave desconocida ignorada: {k}")).collect()))
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!("schemaVersion {} no soportado (esperaba {SCHEMA_VERSION})", self.schema_version));
        }
        if self.name.trim().is_empty() {
            return Err("name vacío".into());
        }
        let l = &self.look;
        for (name, v) in [("Zoom", l.zoom), ("Exposure", l.exposure), ("FocalDistance", l.focal_distance), ("Aperture", l.aperture)] {
            if let Some(v) = v.filter(|v| !protocol::in_range(name, *v)) {
                return Err(format!("{name} fuera de rango: {v}"));
            }
        }
        if let Some(f) = &self.frame {
            check("distance", f.distance, 0.1, 20.0)?;
            check("height", f.height, -3.0, 5.0)?;
            check("yaw", f.yaw, -180.0, 180.0)?;
            check("pitch", f.pitch, -90.0, 90.0)?;
            check("roll", f.roll, -180.0, 180.0)?;
        }
        Ok(())
    }

    /// Comandos para aplicar el preset. `exposure` es la actual, para los modificadores.
    pub fn commands(&self, anchor: Option<&Anchor>, exposure: f32) -> Result<Vec<Cmd>, String> {
        let l = &self.look;
        let mut c = Vec::new();
        for (n, v) in [("LookAtMe", l.look_at_me), ("AutoLevelPitch", l.auto_level_pitch), ("AutoLevelRoll", l.auto_level_roll)] {
            c.extend(v.map(|b| Cmd::Set(n, Value::B(b))));
        }
        if let Some(f) = &self.frame {
            c.push(Cmd::Pose(anchor.ok_or("sin ancla: calibra primero")?.pose(f)));
        }
        for (n, v) in [("Zoom", l.zoom), ("FocalDistance", l.focal_distance), ("Aperture", l.aperture)] {
            c.extend(v.map(|v| Cmd::Set(n, Value::F(v))));
        }
        c.extend(l.exposure.map(|e| Cmd::Set("Exposure", Value::F(if self.modifier { exposure + e } else { e }))));
        Ok(c)
    }

    pub fn apply(&self, cam: &Camera, anchor: Option<&Anchor>) -> Result<(), String> {
        let exposure = match cam.state.lock().unwrap().values.get("Exposure") {
            Some(Value::F(v)) => *v,
            _ => 0.0,
        };
        for c in self.commands(anchor, exposure)? {
            match c {
                Cmd::Set(n, v) => cam.set(n, v)?,
                Cmd::Pose(p) => cam.pose(&p)?,
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Set(&'static str, Value),
    Pose(Pose),
}

/// Presets incluidos en el binario.
pub fn seeds() -> Vec<Preset> {
    let raw: Vec<Json> = serde_json::from_str(SEEDS).expect("seeds.json inválido");
    raw.iter().map(|j| Preset::from_json(&j.to_string()).expect("semilla inválida").0).collect()
}

fn slug(name: &str) -> String {
    name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect()
}

pub fn save(dir: &Path, p: &Preset) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{}.json", slug(&p.name))), json).map_err(|e| e.to_string())
}

/// Presets del usuario en `dir`; los archivos inválidos se saltan y quedan como avisos.
pub fn load_user(dir: &Path) -> (Vec<Preset>, Vec<String>) {
    let (mut presets, mut warnings) = (Vec::new(), Vec::new());
    let Ok(entries) = std::fs::read_dir(dir) else { return (presets, warnings) };
    for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")) {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| Preset::from_json(&s)) {
            Ok((p, w)) => {
                presets.push(p);
                warnings.extend(w.into_iter().map(|w| format!("{name}: {w}")));
            }
            Err(e) => warnings.push(format!("{name}: {e}")),
        }
    }
    (presets, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(name: &str) -> Preset {
        seeds().into_iter().find(|p| p.name == name).unwrap()
    }

    #[test]
    fn seeds_are_valid() {
        let s = seeds();
        assert_eq!(s.len(), 11);
        assert!(seed("Mundo nocturno").modifier && seed("Mundo nocturno").frame.is_none());
    }

    #[test]
    fn rejects_bad_and_warns_unknown() {
        let base = |extra: &str| format!(r#"{{"schemaVersion":1,"name":"x","category":"retrato","frame":null{extra}}}"#);
        assert!(Preset::from_json(&base(r#","look":{"zoom":500}"#)).is_err());
        assert!(Preset::from_json(&base("").replace(r#""schemaVersion":1"#, r#""schemaVersion":2"#)).is_err());
        let bad_frame = base("").replace("null", r#"{"distance":0,"height":0,"yaw":0,"pitch":0,"roll":0}"#);
        assert!(Preset::from_json(&bad_frame).is_err());
        let (_, w) = Preset::from_json(&base(r#","foo":1,"look":{"bar":2}"#)).unwrap();
        assert_eq!(w, ["clave desconocida ignorada: foo", "clave desconocida ignorada: look.bar"]);
    }

    #[test]
    fn commands_for_portrait_and_modifier() {
        let anchor = Anchor { eye: [0.0, 1.6, 0.0], yaw: 0.0 };
        let c = seed("Retrato ¾").commands(Some(&anchor), 0.0).unwrap();
        assert!(c.contains(&Cmd::Set("Zoom", Value::F(54.0))));
        assert!(c.iter().any(|c| matches!(c, Cmd::Pose(_))));
        assert!(seed("Retrato ¾").commands(None, 0.0).is_err());
        // el modificador no mueve la cámara y suma a la exposición actual
        let n = seed("Mundo nocturno").commands(None, -2.0).unwrap();
        assert_eq!(n, vec![Cmd::Set("Exposure", Value::F(-1.0))]);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("velza-test-{}", std::process::id()));
        let p = seed("Grupal");
        save(&dir, &p).unwrap();
        std::fs::write(dir.join("roto.json"), "{").unwrap();
        let (loaded, warnings) = load_user(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(loaded, vec![p]);
        assert_eq!(warnings.len(), 1);
    }
}
