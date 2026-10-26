//! Iris Studio: captura de keyframes, evaluación determinista, motor cinematográfico,
//! calibración técnica/visual, compatibilidad Camera Dolly, grabación continua y persistencia.

pub mod calibration;
pub mod dolly;
pub mod evaluator;
pub mod kinematics;
pub mod math3d;
pub mod model;
pub mod player;
pub mod recorder;

use crate::anchor::Anchor;
use crate::osc::protocol::Pose;
use model::{
    CalibrationConfig, CinematicConfig, CinematicProfile, DurationInputMode, DurationUnit, EffectClip, EffectKind,
    Keyframe, KnotMode, LookTrackPoint, ReferenceSpace, SequenceProject, Transition, STUDIO_SCHEMA_VERSION,
};
use std::path::Path;

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = s.trim_matches('-');
    if trimmed.is_empty() { "secuencia".into() } else { trimmed.into() }
}

/// Secuencias semilla demostrativas (relativas al ancla) para que el usuario pueda explorar Iris Studio de inmediato.
pub fn starter_sequences() -> Vec<SequenceProject> {
    let default_anchor = Anchor { eye: [0.0, 1.65, 0.0], yaw: 0.0 };
    let mk_kf = |id: &str,
                 label: &str,
                 time: f32,
                 local_pos: [f32; 3],
                 local_rot: [f32; 3],
                 zoom: Option<f32>,
                 knot_mode: KnotMode| {
        let local = Pose { pos: local_pos, rot: local_rot };
        Keyframe {
            id: id.into(),
            label: label.into(),
            time,
            raw_pose: default_anchor.local_to_world(&local, 1.0),
            local_pose: Some(local),
            captured_at_ms: 0,
            transition: Transition::Smooth,
            custom_curve: None,
            knot_mode,
            look: LookTrackPoint {
                zoom,
                exposure: None,
                focal_distance: None,
                aperture: None,
            },
        }
    };

    let mut intro = SequenceProject {
        schema_version: STUDIO_SCHEMA_VERSION,
        id: "seed-arco-heroico".into(),
        name: "Arco Heroico Cinematográfico".into(),
        duration: 10.0,
        duration_unit: DurationUnit::Seconds,
        duration_input_mode: DurationInputMode::ValueUnit,
        loop_playback: false,
        space: ReferenceSpace::Anchor,
        anchor_snapshot: Some(default_anchor),
        path_lock: true,
        trajectory_scale: 1.0,
        cinematic: CinematicConfig::for_profile(CinematicProfile::Cinematic),
        calibration: CalibrationConfig::default(),
        target: None,
        keyframes: vec![
            mk_kf("kf-a", "Punto A", 0.0, [-1.2, -0.35, 1.8], [-11.0, 146.0, 0.0], Some(38.0), KnotMode::Stop),
            mk_kf("kf-b", "Punto B", 5.0, [0.0, 0.05, 1.35], [2.0, 180.0, 0.0], Some(54.0), KnotMode::PassThrough),
            mk_kf("kf-c", "Punto C", 10.0, [1.2, 0.25, 1.9], [7.5, -148.0, 0.0], Some(45.0), KnotMode::Stop),
        ],
        effects: vec![EffectClip {
            id: "ef-1".into(),
            kind: EffectKind::CameraBreathing,
            track: 0,
            start_time: 0.0,
            duration: 10.0,
            intensity: 0.8,
            speed: 1.0,
            param_a: None,
            param_b: None,
            easing: Transition::Smooth,
            custom_curve: None,
            enabled: true,
        }],
        raw_recording: Vec::new(),
    };
    intro.sort_keyframes();

    let mut reveal = SequenceProject {
        schema_version: STUDIO_SCHEMA_VERSION,
        id: "seed-reveal-dramatico".into(),
        name: "Reveal Dramático + Zoom".into(),
        duration: 8.0,
        duration_unit: DurationUnit::Seconds,
        duration_input_mode: DurationInputMode::ValueUnit,
        loop_playback: false,
        space: ReferenceSpace::Anchor,
        anchor_snapshot: Some(default_anchor),
        path_lock: true,
        trajectory_scale: 1.0,
        cinematic: CinematicConfig::for_profile(CinematicProfile::Organic),
        calibration: CalibrationConfig::default(),
        target: None,
        keyframes: vec![
            mk_kf("kf-a", "Punto A", 0.0, [0.25, 0.05, 0.85], [3.0, -163.0, -4.0], Some(62.0), KnotMode::Stop),
            mk_kf("kf-b", "Punto B", 4.0, [-0.4, -0.1, 1.8], [-3.0, 167.0, 0.0], Some(48.0), KnotMode::PassThrough),
            mk_kf("kf-c", "Punto C", 8.0, [0.0, 0.35, 3.2], [6.0, 180.0, 0.0], Some(28.0), KnotMode::Stop),
        ],
        effects: vec![
            EffectClip {
                id: "ef-zoom".into(),
                kind: EffectKind::SmoothZoom,
                track: 0,
                start_time: 1.5,
                duration: 5.0,
                intensity: 1.0,
                speed: 1.0,
                param_a: Some(60.0),
                param_b: Some(32.0),
                easing: Transition::SmoothInOut,
                custom_curve: None,
                enabled: true,
            },
            EffectClip {
                id: "ef-hand".into(),
                kind: EffectKind::Handheld,
                track: 1,
                start_time: 0.0,
                duration: 8.0,
                intensity: 0.7,
                speed: 1.0,
                param_a: None,
                param_b: None,
                easing: Transition::Linear,
                custom_curve: None,
                enabled: true,
            },
        ],
        raw_recording: Vec::new(),
    };
    reveal.sort_keyframes();

    vec![intro, reveal]
}

pub fn save_sequence(dir: &Path, project: &SequenceProject) -> Result<(), String> {
    project.validate()?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file_id = if project.id.trim().is_empty() {
        slug(&project.name)
    } else {
        slug(&project.id)
    };
    let json = serde_json::to_string_pretty(project).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{file_id}.iris-seq.json")), json).map_err(|e| e.to_string())
}

pub fn delete_sequence(dir: &Path, id: &str) -> Result<(), String> {
    let path = dir.join(format!("{}.iris-seq.json", slug(id)));
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn import_sequence_json(raw: &str) -> Result<SequenceProject, String> {
    let mut p: SequenceProject = serde_json::from_str(raw).map_err(|e| format!("JSON inválido: {e}"))?;
    p.sort_keyframes();
    p.validate()?;
    Ok(p)
}

pub fn load_sequences(dir: &Path) -> (Vec<SequenceProject>, Vec<String>) {
    let mut list = starter_sequences();
    let mut warnings = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (list, warnings);
    };
    for path in entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
    {
        let fname = path.file_name().unwrap().to_string_lossy().into_owned();
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| import_sequence_json(&s))
        {
            Ok(seq) => {
                if let Some(pos) = list.iter().position(|x| x.id == seq.id) {
                    list[pos] = seq;
                } else {
                    list.push(seq);
                }
            }
            Err(e) => warnings.push(format!("{fname}: {e}")),
        }
    }
    (list, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_sequences_are_valid_and_roundtrip_on_disk() {
        let seeds = starter_sequences();
        assert_eq!(seeds.len(), 2);
        for s in &seeds {
            assert!(s.validate().is_ok());
        }
        let tmp = std::env::temp_dir().join(format!("velza-seq-test-{}", std::process::id()));
        let mut custom = seeds[0].clone();
        custom.id = "mi-secuencia".into();
        custom.name = "Mi Secuencia Personal".into();
        custom.duration = 90.0; // 1.5 minutos
        custom.duration_unit = DurationUnit::Minutes;
        custom.duration_input_mode = DurationInputMode::MmSs;
        save_sequence(&tmp, &custom).unwrap();

        let (loaded, warnings) = load_sequences(&tmp);
        assert!(warnings.is_empty());
        let saved_seq = loaded.iter().find(|x| x.id == "mi-secuencia").unwrap();
        assert_eq!(saved_seq.duration, 90.0);
        assert_eq!(saved_seq.duration_unit, DurationUnit::Minutes);
        assert_eq!(saved_seq.duration_input_mode, DurationInputMode::MmSs);

        delete_sequence(&tmp, "mi-secuencia").unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
