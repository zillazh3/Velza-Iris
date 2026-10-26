//! Pruebas unitarias completas para el protocolo y servicio del Rig Visualizador de VRChat.

use super::config::VisualizerConfig;
use super::protocol::*;
use super::service::{MuxTiming, SkipReason, SyncOutcome, VisualizerService};
use crate::anchor::Anchor;
use crate::osc::protocol::Pose;
use crate::studio::model::{Keyframe, SequenceProject};
use rosc::OscType;
use std::sync::{Arc, Mutex};

#[test]
fn test_01_all_12_endpoints_match_strictly() {
    assert_eq!(ALL_ENDPOINTS.len(), 12);

    assert_eq!(ADDR_IRIS_ENABLE, "/avatar/parameters/iris_enable");
    assert_eq!(ADDR_IRIS_CAM_X, "/avatar/parameters/iris_cam_x");
    assert_eq!(ADDR_IRIS_CAM_Y, "/avatar/parameters/iris_cam_y");
    assert_eq!(ADDR_IRIS_CAM_Z, "/avatar/parameters/iris_cam_z");
    assert_eq!(ADDR_IRIS_CAM_RX, "/avatar/parameters/iris_cam_rx");
    assert_eq!(ADDR_IRIS_CAM_RY, "/avatar/parameters/iris_cam_ry");
    assert_eq!(ADDR_IRIS_CAM_RZ, "/avatar/parameters/iris_cam_rz");
    assert_eq!(ADDR_IRIS_WP_INDEX, "/avatar/parameters/iris_wp_index");
    assert_eq!(ADDR_IRIS_WP_X, "/avatar/parameters/iris_wp_x");
    assert_eq!(ADDR_IRIS_WP_Y, "/avatar/parameters/iris_wp_y");
    assert_eq!(ADDR_IRIS_WP_Z, "/avatar/parameters/iris_wp_z");
    assert_eq!(ADDR_IRIS_HIDE_CAM, "/avatar/parameters/iris_hide_cam");

    // Verificar que todos los endpoints comiencen con /avatar/parameters/iris_
    for ep in ALL_ENDPOINTS {
        assert!(ep.starts_with("/avatar/parameters/iris_"));
    }
}

#[test]
fn test_02_position_normalization_and_clamp() {
    let anchor = [10.0, 2.0, -5.0];

    // En el ancla exacto -> 0.0
    let norm_origin = normalize_position(anchor, anchor);
    assert_eq!(norm_origin, [0.0, 0.0, 0.0]);

    // +25m en X -> +0.5
    let p_half = [35.0, 2.0, -5.0];
    let norm_half = normalize_position(p_half, anchor);
    assert!((norm_half[0] - 0.5).abs() < 1e-5);
    assert_eq!(norm_half[1], 0.0);
    assert_eq!(norm_half[2], 0.0);

    // +50m en Y -> +1.0
    let p_max = [10.0, 52.0, -5.0];
    let norm_max = normalize_position(p_max, anchor);
    assert!((norm_max[1] - 1.0).abs() < 1e-5);

    // -50m en Z -> -1.0
    let p_min = [10.0, 2.0, -55.0];
    let norm_min = normalize_position(p_min, anchor);
    assert!((norm_min[2] - (-1.0)).abs() < 1e-5);

    // Superar rango (+75m) -> clamp a +1.0
    let p_overflow = [85.0, 2.0, -5.0];
    let norm_overflow = normalize_position(p_overflow, anchor);
    assert_eq!(norm_overflow[0], 1.0);

    // Superar rango (-100m) -> clamp a -1.0
    let p_underflow = [-90.0, 2.0, -5.0];
    let norm_underflow = normalize_position(p_underflow, anchor);
    assert_eq!(norm_underflow[0], -1.0);

    // Manejo seguro de valores no finitos (NaN, Inf) -> 0.0
    let p_nan = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
    let norm_nan = normalize_position(p_nan, anchor);
    assert_eq!(norm_nan, [0.0, 0.0, 0.0]);
}

#[test]
fn test_03_euler_angle_wrapping_and_normalization() {
    // 0° -> 0.0
    assert_eq!(normalize_euler_angle(0.0), 0.0);

    // 90° -> 0.5
    assert!((normalize_euler_angle(90.0) - 0.5).abs() < 1e-5);

    // -90° -> -0.5
    assert!((normalize_euler_angle(-90.0) - (-0.5)).abs() < 1e-5);

    // 270° (equivalente a -90°) -> -0.5
    assert!((normalize_euler_angle(270.0) - (-0.5)).abs() < 1e-5);

    // 450° (equivalente a 90°) -> 0.5
    assert!((normalize_euler_angle(450.0) - 0.5).abs() < 1e-5);

    // 180° -> -1.0
    assert!((normalize_euler_angle(180.0).abs() - 1.0).abs() < 1e-5);

    // -180° -> -1.0
    assert!((normalize_euler_angle(-180.0) - (-1.0)).abs() < 1e-5);

    // -270° (equivalente a 90°) -> 0.5
    assert!((normalize_euler_angle(-270.0) - 0.5).abs() < 1e-5);

    // NaN / Inf -> 0.0
    assert_eq!(normalize_euler_angle(f32::NAN), 0.0);
    assert_eq!(normalize_euler_angle(f32::INFINITY), 0.0);

    // Rotación 3D completa
    let rot = normalize_rotation([45.0, -90.0, 180.0]);
    assert!((rot[0] - 0.25).abs() < 1e-5);
    assert!((rot[1] - (-0.5)).abs() < 1e-5);
    assert!((rot[2].abs() - 1.0).abs() < 1e-5);
}

#[test]
fn test_04_multiplexer_execution_sequence() {
    let sent_log = Arc::new(Mutex::new(Vec::<(String, Vec<OscType>)>::new()));
    let sent_log_clone = sent_log.clone();

    let sink = move |addr: String, args: Vec<OscType>| -> Result<(), String> {
        sent_log_clone.lock().unwrap().push((addr, args));
        Ok(())
    };

    let anchor = [0.0, 0.0, 0.0];
    let samples = [
        [0.0, 0.0, 5.0],   // W1 -> z=0.1
        [0.0, 0.0, 10.0],  // W2 -> z=0.2
        [0.0, 0.0, 15.0],  // W3 -> z=0.3
        [0.0, 0.0, 20.0],  // W4 -> z=0.4
        [0.0, 0.0, 25.0],  // W5 -> z=0.5
    ];

    VisualizerService::execute_path_mesh_multiplexing(samples, anchor, MuxTiming::LOCAL, &sink).unwrap();

    let log = sent_log.lock().unwrap().clone();

    // 1 mensaje inicial (wp_index=0) + 5 huesos * 5 mensajes (wp_x, wp_y, wp_z, wp_index=i, wp_index=0) = 26 mensajes
    assert_eq!(log.len(), 26);

    // Paso 1: iris_wp_index = 0
    assert_eq!(log[0].0, ADDR_IRIS_WP_INDEX);
    assert_eq!(log[0].1, vec![OscType::Int(0)]);

    // Verificar cada bloque de hueso
    for i in 1..=5 {
        let base_idx = 1 + (i - 1) * 5;

        // a & b: wp_x, wp_y, wp_z
        assert_eq!(log[base_idx].0, ADDR_IRIS_WP_X);
        assert_eq!(log[base_idx + 1].0, ADDR_IRIS_WP_Y);
        assert_eq!(log[base_idx + 2].0, ADDR_IRIS_WP_Z);

        let expected_z = (i as f32 * 5.0) / 50.0;
        let OscType::Float(actual_z) = log[base_idx + 2].1[0] else { panic!("not float") };
        assert!((actual_z - expected_z).abs() < 1e-5);

        // d: wp_index = i
        assert_eq!(log[base_idx + 3].0, ADDR_IRIS_WP_INDEX);
        assert_eq!(log[base_idx + 3].1, vec![OscType::Int(i as i32)]);

        // f: wp_index = 0
        assert_eq!(log[base_idx + 4].0, ADDR_IRIS_WP_INDEX);
        assert_eq!(log[base_idx + 4].1, vec![OscType::Int(0)]);
    }
}

#[test]
fn test_05_flow_a_set_and_reset_anchor() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let sent_log = Arc::new(Mutex::new(Vec::<(String, Vec<OscType>)>::new()));
    let sent_log_clone = sent_log.clone();

    let sink: Arc<dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync> =
        Arc::new(move |addr: String, args: Vec<OscType>| -> Result<(), String> {
            sent_log_clone.lock().unwrap().push((addr, args));
            Ok(())
        });

    // 1. Activar Ancla (primera vez: enciende el rig)
    let anchor = Anchor { eye: [1.2, 1.65, 3.4], yaw: 30.0 };
    let res = service.set_anchor(anchor, sink.clone()).unwrap();
    assert_eq!(res, anchor);
    assert!(service.enabled.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(*service.anchor_position.lock().unwrap(), Some(anchor.eye));
    assert_eq!(service.anchor(), Some(anchor));

    let log1 = sent_log.lock().unwrap().clone();
    assert_eq!(log1.len(), 1);
    assert_eq!(log1[0].0, ADDR_IRIS_ENABLE);
    assert_eq!(log1[0].1, vec![OscType::Bool(true)]);

    // 2. Re-calibrar con el rig ya activo: NUNCA se apaga iris_enable (soltaría y re-congelaría Iris_World_Root,
    // que saltaría de sitio y dejaría de coincidir con el gizmo sobre el que se calibró). Solo se REenvía `true`,
    // que es un no-op en VRChat si ya estaba encendido y repara el caso en que el avatar se recargó (arranca en false).
    sent_log.lock().unwrap().clear();
    let new_anchor = Anchor { eye: [2.0, 1.7, 4.0], yaw: -45.0 };
    assert_eq!(service.set_anchor(new_anchor, sink.clone()).unwrap(), new_anchor);
    let log_re = sent_log.lock().unwrap().clone();
    assert_eq!(log_re.len(), 1);
    assert_eq!((log_re[0].0.as_str(), log_re[0].1.clone()), (ADDR_IRIS_ENABLE, vec![OscType::Bool(true)]));
    assert_eq!(service.anchor(), Some(new_anchor));

    // 3. Resetear Ancla
    sent_log.lock().unwrap().clear();
    service.reset_anchor(&*sink).unwrap();
    assert!(!service.enabled.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(*service.anchor_position.lock().unwrap(), None);
    assert_eq!(service.anchor(), None);

    let log3 = sent_log.lock().unwrap().clone();
    assert_eq!(log3.len(), 2);
    assert_eq!(log3[0].0, ADDR_IRIS_ENABLE);
    assert_eq!(log3[0].1, vec![OscType::Bool(false)]);
    assert_eq!(log3[1].0, ADDR_IRIS_WP_INDEX);
    assert_eq!(log3[1].1, vec![OscType::Int(0)]);
}

#[test]
fn test_06_flow_c_camera_bundle_sync() {
    let service = VisualizerService::new(VisualizerConfig::default());
    *service.anchor_position.lock().unwrap() = Some([0.0, 0.0, 0.0]);
    service.enabled.store(true, std::sync::atomic::Ordering::SeqCst);

    let captured_bundle = Arc::new(Mutex::new(Vec::<rosc::OscMessage>::new()));
    let cap_clone = captured_bundle.clone();

    let bundle_sink = move |msgs: Vec<rosc::OscMessage>| -> Result<(), String> {
        *cap_clone.lock().unwrap() = msgs;
        Ok(())
    };

    let cam_pose = Pose {
        pos: [25.0, -10.0, 50.0],
        rot: [45.0, 90.0, 0.0],
    };

    service.sync_camera_frame(&cam_pose, &bundle_sink).unwrap();

    let msgs = captured_bundle.lock().unwrap().clone();
    assert_eq!(msgs.len(), 6);

    assert_eq!(msgs[0].addr, ADDR_IRIS_CAM_X);
    assert_eq!(msgs[0].args, vec![OscType::Float(0.5)]);

    assert_eq!(msgs[1].addr, ADDR_IRIS_CAM_Y);
    assert_eq!(msgs[1].args, vec![OscType::Float(-0.2)]);

    assert_eq!(msgs[2].addr, ADDR_IRIS_CAM_Z);
    assert_eq!(msgs[2].args, vec![OscType::Float(1.0)]);

    assert_eq!(msgs[3].addr, ADDR_IRIS_CAM_RX);
    assert_eq!(msgs[3].args, vec![OscType::Float(0.25)]);

    assert_eq!(msgs[4].addr, ADDR_IRIS_CAM_RY);
    assert_eq!(msgs[4].args, vec![OscType::Float(0.5)]);

    assert_eq!(msgs[5].addr, ADDR_IRIS_CAM_RZ);
    assert_eq!(msgs[5].args, vec![OscType::Float(0.0)]);
}

#[test]
fn test_07_sample_5_waypoints_equidistant() {
    let make_kf = |id: &str, time: f32, pos: [f32; 3]| Keyframe {
        id: id.into(),
        label: id.into(),
        time,
        raw_pose: Pose { pos, rot: [0.0; 3] },
        local_pose: None,
        captured_at_ms: 0,
        transition: crate::studio::model::Transition::Linear,
        custom_curve: None,
        knot_mode: crate::studio::model::KnotMode::PassThrough,
        look: crate::studio::model::LookTrackPoint::default(),
    };

    let mut project = SequenceProject::new("test-proj", "Test", 4.0);
    // 3 keyframes
    project.keyframes.push(make_kf("k0", 0.0, [0.0, 0.0, 0.0]));
    project.keyframes.push(make_kf("k1", 2.0, [10.0, 0.0, 0.0]));
    project.keyframes.push(make_kf("k2", 4.0, [20.0, 0.0, 0.0]));

    let samples = VisualizerService::sample_5_waypoints(&project, None, [0.0, 0.0, 0.0]);
    assert_eq!(samples.len(), 5);

    // t=0.0 -> x=0
    assert!((samples[0][0] - 0.0).abs() < 1e-3);
    // t=0.25 -> t=1.0s -> x=5.0
    assert!((samples[1][0] - 5.0).abs() < 1e-3);
    // t=0.50 -> t=2.0s -> x=10.0
    assert!((samples[2][0] - 10.0).abs() < 1e-3);
    // t=0.75 -> t=3.0s -> x=15.0
    assert!((samples[3][0] - 15.0).abs() < 1e-3);
    // t=1.00 -> t=4.0s -> x=20.0
    assert!((samples[4][0] - 20.0).abs() < 1e-3);
}

#[test]
fn test_08_queue_sync_path_mesh_integration() {
    use std::time::Duration;

    let service = VisualizerService::new(local_config());
    let sent_log = Arc::new(Mutex::new(Vec::<(String, Vec<OscType>)>::new()));
    let sent_log_clone = sent_log.clone();

    let sink: Arc<dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync> =
        Arc::new(move |addr: String, args: Vec<OscType>| -> Result<(), String> {
            sent_log_clone.lock().unwrap().push((addr, args));
            Ok(())
        });

    service.set_anchor(Anchor { eye: [10.0, 1.5, -5.0], yaw: 0.0 }, sink.clone()).unwrap();

    let project = SequenceProject::new("test", "Test", 5.0);
    service.queue_sync_path_mesh(&project, None, sink.clone()).unwrap();

    // Esperar a que el worker complete la multiplexación (tarda ~425ms)
    std::thread::sleep(Duration::from_millis(600));

    assert!(!service.is_syncing_path.load(std::sync::atomic::Ordering::SeqCst));
    assert!(service.last_error.lock().unwrap().is_none());

    let log = sent_log.lock().unwrap().clone();
    // 1 mensaje set_anchor + 26 mensajes multiplexor = 27
    assert_eq!(log.len(), 27);
}


/// Config con los tiempos locales del multiplexor para que los tests de integración no tarden ~3 s.
fn local_config() -> VisualizerConfig {
    VisualizerConfig { mux_timing: MuxTiming::LOCAL, ..VisualizerConfig::default() }
}

type Log = Arc<Mutex<Vec<(String, Vec<OscType>)>>>;

fn logging_sink() -> (Log, Arc<dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync>) {
    let log: Log = Arc::default();
    let l = log.clone();
    (log, Arc::new(move |addr, args| {
        l.lock().unwrap().push((addr, args));
        Ok(())
    }))
}

fn floats(msgs: &[rosc::OscMessage]) -> Vec<f32> {
    msgs.iter()
        .map(|m| match m.args[0] {
            OscType::Float(f) => f,
            _ => panic!("no es float"),
        })
        .collect()
}

#[test]
fn test_09_activate_shows_gizmo_anchored_at_the_world_origin() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let (log, sink) = logging_sink();

    assert_eq!(service.anchor(), None, "apagado: sin ancla");
    service.activate(sink.clone()).unwrap();
    assert!(service.enabled.load(std::sync::atomic::Ordering::SeqCst));
    // El rig vive en el origen del mundo: el ancla es (0,0,0) sin calibrar.
    assert_eq!(service.anchor(), Some(Anchor { eye: [0.0; 3], yaw: 0.0 }));
    let l = log.lock().unwrap().clone();
    assert_eq!(l.len(), 7, "enable + 6 params de cámara en 0");
    assert_eq!((l[0].0.as_str(), l[0].1.clone()), (ADDR_IRIS_ENABLE, vec![OscType::Bool(true)]));
    for (addr, args) in &l[1..] {
        assert!(addr.starts_with("/avatar/parameters/iris_cam_"));
        assert_eq!(args, &vec![OscType::Float(0.0)]);
    }

    // Acción explícita del usuario: siempre reenvía (si el avatar se recargó, iris_enable volvió a false en VRChat
    // aunque la app siga creyendo que está encendido).
    log.lock().unwrap().clear();
    service.activate(sink).unwrap();
    assert_eq!(log.lock().unwrap().len(), 7);

    // Sin calibrar: la pose de mundo de la cámara es directamente la del gizmo (5 m en x = 5/50 del rango).
    let out = Arc::new(Mutex::new(vec![]));
    let frame_sink = msgs_sink(out.clone());
    service.sync_camera_frame(&Pose { pos: [5.0, 1.0, -5.0], rot: [0.0, 90.0, 0.0] }, &frame_sink).unwrap();
    let v = floats(&out.lock().unwrap());
    for (i, (got, exp)) in v.iter().zip([0.1, 0.02, -0.1, 0.0, 0.5, 0.0]).enumerate() {
        assert!((got - exp).abs() < 1e-5, "param {i}: {got} != {exp}");
    }

    // Reenviar el gizmo (p. ej. tras recargar el avatar) vuelve a mandar la pose aunque la cámara no se haya movido.
    out.lock().unwrap().clear();
    service.activate(logging_sink().1).unwrap();
    service.sync_camera_frame(&Pose { pos: [5.0, 1.0, -5.0], rot: [0.0, 90.0, 0.0] }, &frame_sink).unwrap();
    assert_eq!(out.lock().unwrap().len(), 6);
}

#[test]
fn test_10_anchor_yaw_rotates_position_and_rotation_into_rig_axes() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let (_, sink) = logging_sink();
    // Ancla con yaw 90°: "adelante" del rig = +X de mundo, "derecha" = -Z de mundo.
    service.set_anchor(Anchor { eye: [10.0, 1.5, -5.0], yaw: 90.0 }, sink).unwrap();

    let out = Arc::new(Mutex::new(vec![]));
    let o = out.clone();
    let frame_sink = move |m: Vec<rosc::OscMessage>| -> Result<(), String> {
        *o.lock().unwrap() = m;
        Ok(())
    };
    // 1 m hacia +X de mundo (= adelante del rig) y 30° más de yaw que el ancla.
    service
        .sync_camera_frame(&Pose { pos: [11.0, 1.5, -5.0], rot: [10.0, 120.0, 5.0] }, &frame_sink)
        .unwrap();
    let v = floats(&out.lock().unwrap());
    let want = [0.0, 0.0, 1.0 / 50.0, 10.0 / 180.0, 30.0 / 180.0, 5.0 / 180.0];
    for (i, (got, exp)) in v.iter().zip(want).enumerate() {
        assert!((got - exp).abs() < 1e-5, "param {i}: {got} != {exp}");
    }
}

#[test]
fn test_11_vrchat_pose_echo_drives_the_same_mesh_params_as_the_commanded_pose() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let (_, sink) = logging_sink();
    service.set_anchor(Anchor { eye: [1.0, 1.6, 2.0], yaw: 0.0 }, sink).unwrap();

    let capture = |pose: &Pose| {
        let o = Arc::new(Mutex::new(vec![]));
        let o2 = o.clone();
        service.last_cam_pose.lock().unwrap().take(); // evita el deadband entre capturas
        service
            .sync_camera_frame(pose, &move |m| {
                *o2.lock().unwrap() = m;
                Ok(())
            })
            .unwrap();
        let v = floats(&o.lock().unwrap());
        v
    };

    let commanded = Pose { pos: [3.0, 1.2, 4.0], rot: [-10.0, -90.0, 10.0] };
    // Lo que VRChat ecoa por 9001: ángulos en [0, 360).
    let echo_args: Vec<OscType> = [3.0, 1.2, 4.0, 350.0, 270.0, 10.0].iter().map(|v| OscType::Float(*v)).collect();
    let echoed = Pose::from_args(&echo_args).unwrap();
    for (a, b) in capture(&commanded).iter().zip(capture(&echoed)) {
        assert!((a - b).abs() < 1e-5);
    }
}

#[test]
fn test_12_path_waypoints_are_expressed_in_anchor_axes() {
    let service = VisualizerService::new(local_config());
    let (log, sink) = logging_sink();
    service.set_anchor(Anchor { eye: [0.0, 0.0, 0.0], yaw: 90.0 }, sink.clone()).unwrap();
    log.lock().unwrap().clear();

    let kf = |id: &str, time: f32| Keyframe {
        id: id.into(),
        label: id.into(),
        time,
        raw_pose: Pose { pos: [0.0, 0.0, 10.0], rot: [0.0; 3] },
        local_pose: None,
        captured_at_ms: 0,
        transition: crate::studio::model::Transition::Linear,
        custom_curve: None,
        knot_mode: crate::studio::model::KnotMode::PassThrough,
        look: crate::studio::model::LookTrackPoint::default(),
    };
    let mut project = SequenceProject::new("p", "P", 4.0);
    project.keyframes.push(kf("a", 0.0));
    project.keyframes.push(kf("b", 4.0));
    service.queue_sync_path_mesh(&project, None, sink).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(700));

    // Mundo +Z 10 m con yaw 90° del ancla: derecha del rig = -Z de mundo → x = -10/50, z = 0.
    let l = log.lock().unwrap().clone();
    let get = |addr: &str| -> f32 {
        match l.iter().find(|(a, _)| a == addr).unwrap().1[0] {
            OscType::Float(f) => f,
            _ => panic!(),
        }
    };
    assert!((get(ADDR_IRIS_WP_X) + 0.2).abs() < 1e-5, "wp_x = {}", get(ADDR_IRIS_WP_X));
    assert!(get(ADDR_IRIS_WP_Z).abs() < 1e-5, "wp_z = {}", get(ADDR_IRIS_WP_Z));
}

#[test]
fn test_13_hide_cam_toggle_sends_bool_and_is_reported() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let (log, sink) = logging_sink();
    service.set_hide_cam(true, &*sink).unwrap();
    assert_eq!(log.lock().unwrap().clone(), vec![(ADDR_IRIS_HIDE_CAM.to_string(), vec![OscType::Bool(true)])]);
    assert!(service.snapshot().hide_cam);
    service.set_hide_cam(false, &*sink).unwrap();
    assert!(!service.snapshot().hide_cam);
}

#[test]
fn test_14_default_mux_timing_is_slow_enough_for_synced_parameters() {
    // Los demás jugadores reciben los params a ~10 Hz: cada fase debe durar más que un par de ciclos de sync.
    let t = VisualizerConfig::default().mux_timing;
    assert_eq!(t, MuxTiming::REMOTE);
    assert!(t.settle_ms >= 200 && t.hold_ms >= 200 && t.freeze_ms >= 100);
    assert!(MuxTiming::LOCAL.hold_ms < t.hold_ms);
}

fn msgs_sink(out: Arc<Mutex<Vec<rosc::OscMessage>>>) -> impl Fn(Vec<rosc::OscMessage>) -> Result<(), String> + Send + Sync {
    move |m| {
        *out.lock().unwrap() = m;
        Ok(())
    }
}

#[test]
fn test_15_sync_outcomes_are_counted_with_their_skip_reason() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let (_, sink) = logging_sink();
    let out = Arc::new(Mutex::new(vec![]));
    let ok_sink = msgs_sink(out.clone());
    let pose = Pose { pos: [1.0, 0.0, 0.0], rot: [0.0; 3] };

    assert_eq!(service.sync_camera_frame(&pose, &ok_sink).unwrap(), SyncOutcome::Disabled);
    service.activate(sink).unwrap();
    assert_eq!(service.sync_camera_frame(&pose, &ok_sink).unwrap(), SyncOutcome::Sent);
    assert_eq!(service.sync_camera_frame(&pose, &ok_sink).unwrap(), SyncOutcome::Deadband);

    // Un sink que falla (p. ej. `camera` ocupada) se cuenta como `busy` y queda como último error.
    let moved = Pose { pos: [2.0, 0.0, 0.0], rot: [0.0; 3] };
    assert!(service.sync_camera_frame(&moved, &|_| Err("cámara ocupada".into())).is_err());
    service.note_pose_echo();
    service.note_skip(SkipReason::ExternalControl);

    let d = service.snapshot().diag;
    assert_eq!(d.frames_sent, 1);
    assert_eq!((d.skipped.disabled, d.skipped.no_anchor, d.skipped.deadband), (1, 0, 1));
    assert_eq!((d.skipped.busy, d.skipped.external_control), (1, 1));
    assert_eq!(d.pose_echoes, 1);
    assert_eq!(d.last_error.as_deref(), Some("cámara ocupada"));
}

#[test]
fn test_16_anchor_nudge_shifts_the_whole_rig_frame_and_resets() {
    let service = VisualizerService::new(local_config());
    let (_, sink) = logging_sink();
    service.set_anchor(Anchor { eye: [0.0; 3], yaw: 0.0 }, sink).unwrap();
    let out = Arc::new(Mutex::new(vec![]));
    let sink_msgs = msgs_sink(out.clone());
    let pose = Pose { pos: [1.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0] };

    service.sync_camera_frame(&pose, &sink_msgs).unwrap();
    assert!((floats(&out.lock().unwrap())[0] - 0.02).abs() < 1e-6);

    // "+0.5 m en X" mueve el gizmo +0.5 m en X del rig, sin esperar a un nuevo eco de pose.
    service.nudge([0.5, 0.0, 0.0], 0.0);
    assert_eq!(service.resend_last(&sink_msgs).unwrap(), SyncOutcome::Sent);
    assert!((floats(&out.lock().unwrap())[0] - 0.03).abs() < 1e-5);
    // +10° de giro lo gira +10° (el giro gira el marco entero, así que también rota la posición: se prueba aparte).
    service.nudge([0.0; 3], 10.0);
    assert_eq!(service.resend_last(&sink_msgs).unwrap(), SyncOutcome::Sent);
    let v = floats(&out.lock().unwrap());
    assert!((v[4] - 10.0 / 180.0).abs() < 1e-5, "ry = {}", v[4]);
    let s = service.snapshot();
    assert_eq!((s.nudge_pos, s.nudge_yaw), ([0.5, 0.0, 0.0], 10.0));

    // Acumula y se limita; reset vuelve al ancla confirmada.
    service.nudge([0.5, 0.0, 0.0], 0.0);
    assert_eq!(service.snapshot().nudge_pos[0], 1.0);
    service.nudge([100.0, 0.0, 0.0], 0.0);
    assert_eq!(service.snapshot().nudge_pos[0], 5.0, "limitado a ±5 m");
    service.reset_nudge();
    assert_eq!(service.resend_last(&sink_msgs).unwrap(), SyncOutcome::Sent);
    assert!((floats(&out.lock().unwrap())[0] - 0.02).abs() < 1e-6);

    // Confirmar un ancla nueva descarta el ajuste fino anterior.
    service.nudge([1.0, 0.0, 0.0], 5.0);
    let (_, sink2) = logging_sink();
    service.set_anchor(Anchor { eye: [0.0; 3], yaw: 0.0 }, sink2).unwrap();
    assert_eq!(service.snapshot().nudge_pos, [0.0; 3]);
}

#[test]
fn test_17_avatar_parameter_echoes_from_vrchat_are_recorded() {
    let service = VisualizerService::new(VisualizerConfig::default());
    service.note_avatar_echo("/avatar/parameters/iris_cam_x", 0.25);
    service.note_avatar_echo("/avatar/parameters/iris_cam_x", 0.5); // el último valor gana
    service.note_avatar_echo("/avatar/parameters/iris_enable", 1.0);
    service.note_avatar_echo("/avatar/parameters/OtroParametro", 9.0); // ajeno al rig: se ignora
    service.note_avatar_echo("/avatar/change", 1.0);

    let mut echoes = service.snapshot().diag.avatar_echo;
    echoes.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(echoes.iter().map(|e| (e.name.as_str(), e.value)).collect::<Vec<_>>(), vec![("iris_cam_x", 0.5), ("iris_enable", 1.0)]);
    assert!(echoes.iter().all(|e| e.age_ms < 5_000));
}

#[test]
fn test_18_avatar_change_resets_the_rig_state_so_it_can_be_shown_again() {
    let service = VisualizerService::new(local_config());
    let (log, sink) = logging_sink();
    service.set_anchor(Anchor { eye: [1.0, 2.0, 3.0], yaw: 40.0 }, sink.clone()).unwrap();
    service.nudge([0.5, 0.0, 0.0], 5.0);
    service.set_hide_cam(true, &*sink).unwrap();
    service.note_avatar_echo("/avatar/parameters/iris_cam_x", 0.3);
    assert!(service.snapshot().enabled);

    // VRChat manda /avatar/change al cargar un avatar: los params iris_* del avatar nuevo arrancan en sus defaults.
    service.on_avatar_changed();
    let s = service.snapshot();
    assert!(!s.enabled, "el rig del avatar nuevo está apagado");
    assert_eq!((s.anchor_position, s.nudge_pos, s.nudge_yaw, s.hide_cam), (None, [0.0; 3], 0.0, false));
    assert!(s.diag.avatar_echo.is_empty());

    // Y mostrarlo otra vez vuelve a mandar iris_enable = true.
    log.lock().unwrap().clear();
    service.activate(sink).unwrap();
    assert_eq!(log.lock().unwrap()[0].1, vec![OscType::Bool(true)]);
    assert!(service.snapshot().enabled);
}

fn project_with(points: &[(f32, [f32; 3])], duration: f32) -> SequenceProject {
    let mut p = SequenceProject::new("p", "P", duration);
    for (i, (time, pos)) in points.iter().enumerate() {
        p.keyframes.push(Keyframe {
            id: format!("k{i}"),
            label: format!("k{i}"),
            time: *time,
            raw_pose: Pose { pos: *pos, rot: [0.0; 3] },
            local_pose: None,
            captured_at_ms: 0,
            transition: crate::studio::model::Transition::Linear,
            custom_curve: None,
            knot_mode: crate::studio::model::KnotMode::PassThrough,
            look: crate::studio::model::LookTrackPoint::default(),
        });
    }
    p
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 0.05)
}

#[test]
fn test_19_path_passes_through_the_keyframes_when_there_are_five_or_fewer() {
    // Keyframes en t = 0, 1.5 y 4 s: el 25/50/75 % del tiempo (1, 2, 3 s) no cae sobre el del medio.
    let kfs = [(0.0, [0.0, 1.0, 0.0]), (1.5, [10.0, 1.0, 5.0]), (4.0, [20.0, 3.0, -5.0])];
    let samples = VisualizerService::sample_5_waypoints(&project_with(&kfs, 4.0), None, [0.0; 3]);
    for (t, pos) in kfs {
        assert!(samples.iter().any(|s| near(*s, pos)), "el path debe pasar por el keyframe t={t} {pos:?}: {samples:?}");
    }
    // Primer y último hueso en los extremos del recorrido, y los 5 en orden temporal (sin retroceder en X).
    assert!(near(samples[0], kfs[0].1) && near(samples[4], kfs[2].1));
    assert!(samples.windows(2).all(|w| w[1][0] >= w[0][0] - 0.05));
}

#[test]
fn test_20_path_with_many_keyframes_keeps_first_last_and_uses_real_keyframes() {
    let kfs: Vec<(f32, [f32; 3])> = (0..8).map(|i| (i as f32, [i as f32 * 3.0, 1.0, (i % 2) as f32 * 4.0])).collect();
    let samples = VisualizerService::sample_5_waypoints(&project_with(&kfs, 7.0), None, [0.0; 3]);
    assert!(near(samples[0], kfs[0].1) && near(samples[4], kfs[7].1));
    // Los 5 huesos caen sobre keyframes reales (no sobre puntos intermedios que cortan las esquinas del zigzag).
    for s in samples {
        assert!(kfs.iter().any(|(_, p)| near(s, *p)), "{s:?} no es un keyframe");
    }
    let distinct = samples.iter().enumerate().filter(|(i, s)| samples[..*i].iter().all(|o| !near(**s, *o))).count();
    assert_eq!(distinct, 5, "los 5 huesos deben ser puntos distintos");
}

#[test]
fn test_21_path_with_two_keyframes_spreads_five_points_along_the_segment() {
    let kfs = [(0.0, [0.0, 0.0, 0.0]), (2.0, [8.0, 0.0, 0.0])];
    let samples = VisualizerService::sample_5_waypoints(&project_with(&kfs, 2.0), None, [0.0; 3]);
    for (i, want) in [0.0_f32, 2.0, 4.0, 6.0, 8.0].iter().enumerate() {
        assert!((samples[i][0] - want).abs() < 0.05, "hueso {i}: x = {} (esperado {want})", samples[i][0]);
    }
}

#[test]
fn test_22_blank_project_collapses_the_path_onto_the_anchor() {
    let blank = SequenceProject::new("b", "Nueva secuencia", 10.0);
    let anchor = [3.0, 1.5, -2.0];
    let samples = VisualizerService::sample_5_waypoints(&blank, None, anchor);
    assert!(samples.iter().all(|p| *p == anchor), "un proyecto vacío no debe dibujar un recorrido inventado: {samples:?}");
}
