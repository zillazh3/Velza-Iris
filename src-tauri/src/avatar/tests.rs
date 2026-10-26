//! Pruebas unitarias para el protocolo y servicio del Rig Visualizador de VRChat.
//!
//! El canal sincronizado se prueba de punta a punta contra un emulador del cliente remoto (`Remote`): cuantiza lo que
//! recibe a 8 bits como la red de VRChat, hace el latch por slot como el `VRCAvatarParameterDriver` del FX y reconstruye
//! el gizmo con la misma matemática que los BlendTrees.

use super::config::VisualizerConfig;
use super::net::{self, NetPath};
use super::protocol::*;
use super::service::{MuxTiming, SkipReason, SyncOutcome, VisualizerService};
use crate::anchor::Anchor;
use crate::osc::protocol::Pose;
use crate::studio::model::{Keyframe, SequenceProject};
use rosc::OscType;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Log = Arc<Mutex<Vec<(String, Vec<OscType>)>>>;
type Sink = Arc<dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync>;

fn local_config() -> VisualizerConfig {
    VisualizerConfig { mux_timing: MuxTiming::LOCAL, ..VisualizerConfig::default() }
}

fn logging_sink() -> (Log, Sink) {
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

fn msgs_sink(out: Arc<Mutex<Vec<rosc::OscMessage>>>) -> impl Fn(Vec<rosc::OscMessage>) -> Result<(), String> + Send + Sync {
    move |m| {
        *out.lock().unwrap() = m;
        Ok(())
    }
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !cond() {
        assert!(t0.elapsed() < Duration::from_secs(5), "timeout esperando: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Espera a que el worker termine un envío: el último mensaje de un trabajo es `iris_net_slot = 0`.
fn wait_job_done(log: &Log, jobs: usize) {
    wait_until("fin del envío por el bus", || {
        log.lock().unwrap().iter().filter(|(a, v)| a == ADDR_IRIS_NET_SLOT && *v == vec![OscType::Int(0)]).count() >= jobs
    });
}

fn float_of(args: &[OscType]) -> f32 {
    match args[0] {
        OscType::Float(f) => f,
        OscType::Int(i) => i as f32,
        OscType::Bool(b) => b as i32 as f32,
        _ => panic!("no es número"),
    }
}

// ----------------------------------------------------------------------------------------------------------------
// Emulador del cliente remoto
// ----------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum NetQuant {
    Round,
    Trunc,
}

struct Remote {
    quant: NetQuant,
    net: [f32; 3],
    pc: [[f32; 3]; NET_POINTS],
    pf: [[f32; 3]; NET_POINTS],
    pr: [[f32; 3]; NET_POINTS],
    reg: [f32; 3],
    hc: [f32; 3],
    hf: [f32; 3],
    hr: [f32; 3],
    path_mode: bool,
    t_net: f32,
    latched: Vec<i32>,
}

impl Remote {
    fn new(quant: NetQuant) -> Remote {
        Remote {
            quant,
            net: [0.0; 3],
            pc: [[0.0; 3]; NET_POINTS],
            pf: [[0.0; 3]; NET_POINTS],
            pr: [[0.0; 3]; NET_POINTS],
            reg: [0.0; 3],
            hc: [0.0; 3],
            hf: [0.0; 3],
            hr: [0.0; 3],
            path_mode: false,
            t_net: 0.0,
            latched: vec![],
        }
    }

    /// Lo que VRChat entrega al remoto para un float sincronizado.
    fn network(&self, x: f32) -> f32 {
        let x = x.clamp(-1.0, 1.0);
        match self.quant {
            NetQuant::Round => (x * NET_QUANT_STEPS).round() / NET_QUANT_STEPS,
            NetQuant::Trunc => (x * NET_QUANT_STEPS).trunc() / NET_QUANT_STEPS,
        }
    }

    fn feed(&mut self, addr: &str, args: &[OscType]) {
        let v = float_of(args);
        match addr {
            ADDR_IRIS_NET_X => self.net[0] = self.network(v),
            ADDR_IRIS_NET_Y => self.net[1] = self.network(v),
            ADDR_IRIS_NET_Z => self.net[2] = self.network(v),
            ADDR_IRIS_PATH_T => self.t_net = (self.network(v) + 1.0) * 0.5,
            ADDR_IRIS_NET_SLOT => self.latch(v as i32),
            _ => {}
        }
    }

    /// Estado `L_k` del FX: `VRCAvatarParameterDriver` Copy de `iris_net_*` (con conversión ×1/254 para los fine).
    fn latch(&mut self, k: i32) {
        if k == 0 {
            return;
        }
        self.latched.push(k);
        let n = self.net;
        let fine = n.map(|x| x / 254.0);
        if k == slot::REGION {
            self.reg = n;
        } else if k == slot::POSE_C {
            self.hc = n;
        } else if k == slot::POSE_F {
            self.hf = fine;
        } else if k == slot::POSE_R {
            self.hr = n;
        } else if k == slot::MODE_PATH {
            self.path_mode = true;
        } else if k == slot::MODE_HAND {
            self.path_mode = false;
        } else {
            for i in 0..NET_POINTS {
                if k == slot::pos_c(i) {
                    self.pc[i] = n;
                } else if k == slot::pos_f(i) {
                    self.pf[i] = fine;
                } else if k == slot::rot_c(i) {
                    self.pr[i] = n;
                }
            }
        }
    }

    fn feed_log(&mut self, log: &Log) {
        for (a, v) in log.lock().unwrap().iter() {
            self.feed(a, v);
        }
    }

    fn region_m(&self) -> [f32; 3] {
        self.reg.map(|r| r * REGION_RANGE_METERS)
    }

    /// Posición reconstruida del punto de control `i` (mundo, m): región + `R·(Pc + Pf)`.
    fn point_pos(&self, i: usize) -> [f32; 3] {
        let r = self.region_m();
        [0, 1, 2].map(|a| r[a] + MAX_RANGE_METERS * join_coarse_fine(self.pc[i][a], self.pf[i][a] * 254.0))
    }

    fn point_rot(&self, i: usize) -> [f32; 3] {
        self.pr[i].map(|x| x * ROT_RANGE_DEGREES)
    }

    /// Gizmo remoto en modo path: `Σ w_i(T)·P_i` con pesos de tienda en los nudos `i/(N-1)` (los 1D BlendTrees interiores).
    fn gizmo_path(&self, t: f32) -> [f32; 3] {
        let x = t.clamp(0.0, 1.0) * (NET_POINTS - 1) as f32;
        let i0 = (x.floor() as usize).min(NET_POINTS - 2);
        let u = x - i0 as f32;
        let (a, b) = (self.point_pos(i0), self.point_pos(i0 + 1));
        [0, 1, 2].map(|k| a[k] * (1.0 - u) + b[k] * u)
    }

    fn hold_pos(&self) -> [f32; 3] {
        let r = self.region_m();
        [0, 1, 2].map(|a| r[a] + MAX_RANGE_METERS * (self.hc[a] + self.hf[a]))
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

// ----------------------------------------------------------------------------------------------------------------
// Protocolo y matemáticas
// ----------------------------------------------------------------------------------------------------------------

#[test]
fn test_01_endpoints_match_strictly() {
    assert_eq!(ADDR_IRIS_ENABLE, "/avatar/parameters/iris_enable");
    assert_eq!(ADDR_IRIS_HIDE_CAM, "/avatar/parameters/iris_hide_cam");
    assert_eq!(ADDR_IRIS_CAM_RZ, "/avatar/parameters/iris_cam_rz");
    assert_eq!(ADDR_IRIS_NET_SLOT, "/avatar/parameters/iris_net_slot");
    assert_eq!(ADDR_IRIS_NET_X, "/avatar/parameters/iris_net_x");
    assert_eq!(ADDR_IRIS_PATH_T, "/avatar/parameters/iris_path_t");
    assert_eq!(addr_pc(3, 1), "/avatar/parameters/iris_pc_3_y");
    assert_eq!(addr_pr(7, 2), "/avatar/parameters/iris_pr_7_z");

    // 7 sincronizados (2 bool + 1 int + 4 float = 42 bits) y 9 + 48 locales del dueño.
    assert_eq!(SYNCED_ENDPOINTS.len(), 7);
    let owner = owner_endpoints();
    assert_eq!(owner.len(), 9 + 2 * 3 * NET_POINTS);
    let mut all: Vec<String> = SYNCED_ENDPOINTS.iter().map(|s| s.to_string()).chain(owner).collect();
    assert!(all.iter().all(|e| e.starts_with("/avatar/parameters/iris_")));
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 7 + 9 + 2 * 3 * NET_POINTS, "sin direcciones repetidas");
}

#[test]
fn test_02_position_normalization_and_clamp() {
    let anchor = [10.0, 2.0, -5.0];
    assert_eq!(normalize_position(anchor, anchor), [0.0, 0.0, 0.0]);
    assert!((normalize_position([35.0, 2.0, -5.0], anchor)[0] - 0.5).abs() < 1e-5);
    assert!((normalize_position([10.0, 52.0, -5.0], anchor)[1] - 1.0).abs() < 1e-5);
    assert!((normalize_position([10.0, 2.0, -55.0], anchor)[2] + 1.0).abs() < 1e-5);
    assert_eq!(normalize_position([85.0, 2.0, -5.0], anchor)[0], 1.0);
    assert_eq!(normalize_position([-90.0, 2.0, -5.0], anchor)[0], -1.0);
    assert_eq!(normalize_position([f32::NAN, f32::INFINITY, f32::NEG_INFINITY], anchor), [0.0, 0.0, 0.0]);
}

#[test]
fn test_03_euler_angle_wrapping_and_normalization() {
    assert_eq!(normalize_euler_angle(0.0), 0.0);
    assert!((normalize_euler_angle(90.0) - 0.5).abs() < 1e-5);
    assert!((normalize_euler_angle(-90.0) + 0.5).abs() < 1e-5);
    assert!((normalize_euler_angle(270.0) + 0.5).abs() < 1e-5);
    assert!((normalize_euler_angle(450.0) - 0.5).abs() < 1e-5);
    assert!((normalize_euler_angle(180.0).abs() - 1.0).abs() < 1e-5);
    assert!((normalize_euler_angle(-180.0) + 1.0).abs() < 1e-5);
    assert!((normalize_euler_angle(-270.0) - 0.5).abs() < 1e-5);
    assert_eq!(normalize_euler_angle(f32::NAN), 0.0);
    assert_eq!(normalize_euler_angle(f32::INFINITY), 0.0);
    let rot = normalize_rotation([45.0, -90.0, 180.0]);
    assert!((rot[0] - 0.25).abs() < 1e-5 && (rot[1] + 0.5).abs() < 1e-5 && (rot[2].abs() - 1.0).abs() < 1e-5);
}

#[test]
fn test_04_coarse_fine_recovers_millimetres_and_survives_round_and_trunc() {
    // Barrido de todo el rango: el coarse solo da 0.39 m; coarse + fine, ~1 mm, incluso con redondeo o truncado de red.
    let mut worst_coarse = 0.0_f32;
    let mut worst_cf = 0.0_f32;
    let mut worst_trunc = 0.0_f32;
    for k in 0..=20_000 {
        let v = -1.0 + 2.0 * k as f32 / 20_000.0;
        let (c, f) = split_coarse_fine(v);
        worst_coarse = worst_coarse.max((v - c).abs() * MAX_RANGE_METERS);
        worst_cf = worst_cf.max((v - join_coarse_fine(c, f)).abs() * MAX_RANGE_METERS);

        // Lo que se ESCRIBE (centro del bin) llega al remoto como c y f exactos con cualquiera de las dos cuantizaciones.
        let (cw, fw) = (net_bin(c), net_bin(f));
        let (round, trunc) = (|x: f32| (x * NET_QUANT_STEPS).round() / NET_QUANT_STEPS, |x: f32| (x * NET_QUANT_STEPS).trunc() / NET_QUANT_STEPS);
        assert!((round(cw) - c).abs() < 1e-6 && (round(fw) - f).abs() < 1e-6, "round: v={v}");
        assert!((trunc(cw) - c).abs() < 1e-6 && (trunc(fw) - f).abs() < 1e-6, "trunc: v={v}");
        worst_trunc = worst_trunc.max((v - join_coarse_fine(trunc(cw), trunc(fw))).abs() * MAX_RANGE_METERS);
    }
    assert!(worst_coarse > 0.19 && worst_coarse < 0.2, "solo coarse: {worst_coarse} m (½ paso = 0.197 m)");
    assert!(worst_cf < 0.002, "coarse + fine: {worst_cf} m");
    assert!(worst_trunc < 0.002, "coarse + fine con truncado de red: {worst_trunc} m");
    // Sin sesgo en 0 ni en los extremos.
    assert_eq!(net_bin(0.0), 0.0);
    assert_eq!(net_bin(1.0), 1.0);
    assert_eq!(net_bin(-1.0), -1.0);
    assert_eq!(q8(f32::NAN), 0.0);
}

#[test]
fn test_05_region_snaps_to_the_grid_the_remote_can_represent() {
    let r = snap_region([123.4, -7.0, 600.0]);
    for a in 0..3 {
        let k = r[a] / REGION_RANGE_METERS * NET_QUANT_STEPS;
        assert!((k - k.round()).abs() < 1e-3, "eje {a}: {} no está en la rejilla", r[a]);
    }
    assert!((r[0] - 123.4).abs() < REGION_RANGE_METERS / NET_QUANT_STEPS / 2.0 + 1e-3);
    assert_eq!(snap_region([0.0; 3]), [0.0; 3]);
}

#[test]
fn test_06_slot_numbers_are_unique_and_fit_in_the_int_parameter() {
    let mut all = vec![slot::REGION, slot::POSE_C, slot::POSE_F, slot::POSE_R, slot::MODE_PATH, slot::MODE_HAND];
    for i in 0..NET_POINTS {
        all.extend([slot::pos_c(i), slot::pos_f(i), slot::rot_c(i)]);
    }
    let n = all.len();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), n, "slots repetidos");
    assert!(all.iter().all(|s| (1..=slot::COUNT).contains(s)) && slot::COUNT < 255);
    assert_eq!(all.len() as i32, slot::COUNT, "los slots son consecutivos desde 1");
}

// ----------------------------------------------------------------------------------------------------------------
// Puntos de control y progreso
// ----------------------------------------------------------------------------------------------------------------

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
    dist(a, b) < 0.05
}

#[test]
fn test_07_control_points_pass_through_the_keyframes() {
    let kfs = [(0.0, [0.0, 1.0, 0.0]), (1.5, [10.0, 1.0, 5.0]), (4.0, [20.0, 3.0, -5.0])];
    let pts = net::sample_control_points(&project_with(&kfs, 4.0), None, [0.0; 3]);
    for (t, pos) in kfs {
        assert!(pts.iter().any(|(_, p)| near(p.pos, pos)), "el path debe pasar por el keyframe t={t} {pos:?}");
    }
    assert!(near(pts[0].1.pos, kfs[0].1) && near(pts[NET_POINTS - 1].1.pos, kfs[2].1));
    assert!(pts.windows(2).all(|w| w[1].0 >= w[0].0), "tiempos no decrecientes");
}

#[test]
fn test_08_many_keyframes_keep_first_last_and_real_keyframes() {
    let kfs: Vec<(f32, [f32; 3])> = (0..12).map(|i| (i as f32, [i as f32 * 3.0, 1.0, (i % 2) as f32 * 4.0])).collect();
    let pts = net::sample_control_points(&project_with(&kfs, 11.0), None, [0.0; 3]);
    assert!(near(pts[0].1.pos, kfs[0].1) && near(pts[NET_POINTS - 1].1.pos, kfs[11].1));
    for (_, p) in &pts {
        assert!(kfs.iter().any(|(_, k)| near(p.pos, *k)), "{:?} no es un keyframe", p.pos);
    }
}

#[test]
fn test_09_two_keyframes_spread_the_points_along_the_segment_and_blank_collapses() {
    let kfs = [(0.0, [0.0, 0.0, 0.0]), (2.0, [14.0, 0.0, 0.0])];
    let pts = net::sample_control_points(&project_with(&kfs, 2.0), None, [0.0; 3]);
    // Se subdivide el tramo más largo hasta completar los 8: extremos fijos, orden creciente y sin huecos de más de 7 m.
    assert!(near(pts[0].1.pos, kfs[0].1) && near(pts[NET_POINTS - 1].1.pos, kfs[1].1));
    assert!(pts.windows(2).all(|w| w[1].1.pos[0] > w[0].1.pos[0] && w[1].1.pos[0] - w[0].1.pos[0] <= 7.01));
    let blank = SequenceProject::new("b", "Nueva secuencia", 10.0);
    let anchor = [3.0, 1.5, -2.0];
    let pts = net::sample_control_points(&blank, None, anchor);
    assert!(pts.iter().all(|(_, p)| p.pos == anchor), "un proyecto vacío no dibuja un recorrido inventado");
}

#[test]
fn test_09b_path_lock_keeps_control_points_when_the_operator_anchor_changes() {
    let kfs = [(0.0, [0.0, 1.0, 0.0]), (1.5, [10.0, 1.0, 5.0]), (4.0, [20.0, 3.0, -5.0])];
    let mut p = project_with(&kfs, 4.0);
    p.bind_anchor(Anchor { eye: [1.0, 1.6, 1.0], yaw: 30.0 });
    let moved = Anchor { eye: [-40.0, 1.7, 22.0], yaw: -120.0 };
    let (a, b) = (net::sample_control_points(&p, None, [0.0; 3]), net::sample_control_points(&p, Some(&moved), [0.0; 3]));
    assert!(a.iter().zip(&b).all(|(x, y)| x.1.pos == y.1.pos), "re-anclar no debe mover los puntos del path");
}

fn straight_path() -> NetPath {
    let mut s = [(0.0, Pose { pos: [0.0; 3], rot: [0.0; 3] }); NET_POINTS];
    for (i, e) in s.iter_mut().enumerate() {
        *e = (i as f32, Pose { pos: [10.0 * i as f32, 1.5, -3.0 * i as f32], rot: [0.0, 30.0 * i as f32, 0.0] });
    }
    NetPath::new(s)
}

#[test]
fn test_10_path_t_follows_time_and_projects_easing_inside_the_segment() {
    let p = straight_path();
    let last = (NET_POINTS - 1) as f32;
    assert_eq!(p.path_t(0.0, p.points[0].pos), 0.0);
    assert!((p.path_t(last, p.points[7].pos) - 1.0).abs() < 1e-6);
    // En un nudo exacto, T = i/(N-1).
    for i in 0..NET_POINTS {
        let t = p.path_t(i as f32, p.points[i].pos);
        assert!((t - i as f32 / last).abs() < 1e-5, "nudo {i}: T = {t}");
    }
    // A mitad de tiempo del segmento 2, pero con la cámara ya al 90 % (ease-out): la proyección manda.
    let (a, b) = (p.points[2].pos, p.points[3].pos);
    let pos = [a[0] + 0.9 * (b[0] - a[0]), a[1] + 0.9 * (b[1] - a[1]), a[2] + 0.9 * (b[2] - a[2])];
    let t = p.path_t(2.5, pos);
    assert!((t - (2.0 + 0.9) / last).abs() < 1e-4, "T = {t}");
    // Un proyecto en blanco (todos los tiempos iguales) no produce NaN.
    let blank = NetPath::new([(0.0, Pose { pos: [1.0; 3], rot: [0.0; 3] }); NET_POINTS]);
    assert!(blank.path_t(0.0, [1.0; 3]).is_finite() && blank.path_t(5.0, [9.0; 3]).is_finite());
}

#[test]
fn test_11_rotations_are_unwrapped_along_the_path() {
    // El yaw cruza ±180° entre el punto 1 y el 2: desenvuelto sigue creciendo en vez de saltar -340°.
    let mut s = [(0.0, Pose { pos: [0.0; 3], rot: [0.0; 3] }); NET_POINTS];
    for (i, e) in s.iter_mut().enumerate() {
        *e = (i as f32, Pose { pos: [i as f32, 0.0, 0.0], rot: [0.0, 100.0 * i as f32, 0.0] });
    }
    let p = NetPath::new(s);
    for i in 1..NET_POINTS {
        assert!((p.points[i].rot[1] - p.points[i - 1].rot[1] - 100.0).abs() < 1e-3, "salto en el punto {i}");
    }
    assert_eq!(p.rot_norm(7)[1], 1.0, "700° se recorta a ±360° (rango del canal)");
}

// ----------------------------------------------------------------------------------------------------------------
// Bus sincronizado de punta a punta
// ----------------------------------------------------------------------------------------------------------------

#[test]
fn test_12_slot_protocol_orders_data_selector_and_ends_idle() {
    let path = straight_path();
    let slots = path.slots();
    assert_eq!(slots.len(), 1 + 3 * NET_POINTS);
    // Orden de prioridad: región, posiciones coarse, rotaciones, posiciones fine.
    assert_eq!(slots[0].slot, slot::REGION);
    assert_eq!(slots[1].slot, slot::pos_c(0));
    assert_eq!(slots[1 + NET_POINTS].slot, slot::rot_c(0));
    assert_eq!(slots[1 + 2 * NET_POINTS].slot, slot::pos_f(0));

    let (log, sink) = logging_sink();
    VisualizerService::execute_slots(&slots[..2], MuxTiming::LOCAL, &*sink).unwrap();
    let l = log.lock().unwrap().clone();
    let addrs: Vec<&str> = l.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        addrs,
        [ADDR_IRIS_NET_X, ADDR_IRIS_NET_Y, ADDR_IRIS_NET_Z, ADDR_IRIS_NET_SLOT, ADDR_IRIS_NET_X, ADDR_IRIS_NET_Y, ADDR_IRIS_NET_Z, ADDR_IRIS_NET_SLOT, ADDR_IRIS_NET_SLOT],
        "datos → selector por slot y selector 0 al final"
    );
    assert_eq!(l[3].1, vec![OscType::Int(slot::REGION)]);
    assert_eq!(l[7].1, vec![OscType::Int(slot::pos_c(0))]);
    assert_eq!(l[8].1, vec![OscType::Int(0)]);
}

#[test]
fn test_13_remote_reconstructs_every_control_point_within_two_millimetres() {
    for quant in [NetQuant::Round, NetQuant::Trunc] {
        let path = straight_path();
        let (log, sink) = logging_sink();
        VisualizerService::execute_slots(&path.slots(), MuxTiming::LOCAL, &*sink).unwrap();
        let mut remote = Remote::new(quant);
        remote.feed_log(&log);

        assert_eq!(remote.region_m(), path.region, "la región llega exacta (está en la rejilla)");
        for i in 0..NET_POINTS {
            let e = dist(remote.point_pos(i), path.points[i].pos);
            assert!(e < 0.002, "punto {i}: error {e} m");
            let r = remote.point_rot(i);
            for a in 0..3 {
                // Rotación solo coarse sobre ±360°: paso 2.83°, error máx. 1.42°.
                assert!((r[a] - path.points[i].rot[a]).abs() <= 1.45, "punto {i} eje {a}: {} vs {}", r[a], path.points[i].rot[a]);
            }
        }
        // El gizmo remoto en los nudos del path coincide con los puntos (T = i/(N-1)).
        for i in 0..NET_POINTS {
            let g = remote.gizmo_path(i as f32 / (NET_POINTS - 1) as f32);
            assert!(dist(g, path.points[i].pos) < 0.002);
        }
    }
}

#[test]
fn test_14_default_bus_timing_is_slow_enough_for_synced_parameters() {
    // Los demás jugadores reciben los params a ~5-10 Hz: datos y selector deben durar más de un par de ciclos de sync.
    let t = VisualizerConfig::default().mux_timing;
    assert_eq!(t, MuxTiming::REMOTE);
    assert!(t.data_ms >= 200 && t.slot_ms >= 200);
    assert!(MuxTiming::LOCAL.slot_ms < t.slot_ms);
}

// ----------------------------------------------------------------------------------------------------------------
// Servicio: flujos del dueño
// ----------------------------------------------------------------------------------------------------------------

#[test]
fn test_15_flow_a_set_and_reset_anchor() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let (log, sink) = logging_sink();

    let anchor = Anchor { eye: [1.2, 1.65, 3.4], yaw: 30.0 };
    assert_eq!(service.set_anchor(anchor, sink.clone()).unwrap(), anchor);
    assert!(service.enabled.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(service.anchor(), Some(anchor));
    assert_eq!(log.lock().unwrap().clone(), vec![(ADDR_IRIS_ENABLE.to_string(), vec![OscType::Bool(true)])]);

    // Re-anclar con el rig activo solo REenvía `true` (no-op en VRChat si ya estaba encendido; repara un avatar recargado).
    log.lock().unwrap().clear();
    let new_anchor = Anchor { eye: [2.0, 1.7, 4.0], yaw: -45.0 };
    assert_eq!(service.set_anchor(new_anchor, sink.clone()).unwrap(), new_anchor);
    assert_eq!(log.lock().unwrap().len(), 1);

    log.lock().unwrap().clear();
    service.reset_anchor(&*sink).unwrap();
    assert!(!service.enabled.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(service.anchor(), None);
    assert_eq!(log.lock().unwrap().clone(), vec![(ADDR_IRIS_ENABLE.to_string(), vec![OscType::Bool(false)])]);
}

#[test]
fn test_16_flow_c_camera_params_are_relative_to_the_region() {
    let service = VisualizerService::new(VisualizerConfig::default());
    *service.anchor_position.lock().unwrap() = Some([0.0, 0.0, 0.0]);
    service.enabled.store(true, std::sync::atomic::Ordering::SeqCst);

    let out = Arc::new(Mutex::new(vec![]));
    let sink = msgs_sink(out.clone());
    service
        .sync_camera_frame(&Pose { pos: [25.0, -10.0, 39.0], rot: [45.0, 90.0, 0.0] }, &sink)
        .unwrap();
    let msgs = out.lock().unwrap().clone();
    let addrs: Vec<&str> = msgs.iter().map(|m| m.addr.as_str()).collect();
    assert_eq!(addrs, [ADDR_IRIS_CAM_X, ADDR_IRIS_CAM_Y, ADDR_IRIS_CAM_Z, ADDR_IRIS_CAM_RX, ADDR_IRIS_CAM_RY, ADDR_IRIS_CAM_RZ]);
    for (got, exp) in floats(&msgs).iter().zip([0.5, -0.2, 0.78, 0.25, 0.5, 0.0]) {
        assert!((got - exp).abs() < 1e-5, "{got} != {exp}");
    }
}

#[test]
fn test_17_activate_shows_gizmo_at_the_region_and_writes_the_owner_params() {
    let service = VisualizerService::new(local_config());
    let (log, sink) = logging_sink();

    assert_eq!(service.anchor(), None, "apagado: sin ancla");
    service.activate(sink.clone()).unwrap();
    assert!(service.enabled.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(service.anchor(), Some(Anchor { eye: [0.0; 3], yaw: 0.0 }));

    wait_job_done(&log, 1);
    let l = log.lock().unwrap().clone();
    assert_eq!((l[0].0.as_str(), l[0].1.clone()), (ADDR_IRIS_ENABLE, vec![OscType::Bool(true)]));
    // 6 cam en 0 y 3 de región, luego el bus (región para los remotos).
    for (addr, args) in &l[1..7] {
        assert!(addr.starts_with("/avatar/parameters/iris_cam_"), "{addr}");
        assert_eq!(args, &vec![OscType::Float(0.0)]);
    }
    assert_eq!(l[7].0, ADDR_IRIS_REG_X);
    assert!(l.iter().any(|(a, v)| a == ADDR_IRIS_NET_SLOT && *v == vec![OscType::Int(slot::REGION)]));

    // Sin calibrar: la pose de mundo es directamente la del gizmo (5 m en x = 5/50 del rango).
    let out = Arc::new(Mutex::new(vec![]));
    let frame_sink = msgs_sink(out.clone());
    service.sync_camera_frame(&Pose { pos: [5.0, 1.0, -5.0], rot: [0.0, 90.0, 0.0] }, &frame_sink).unwrap();
    for (i, (got, exp)) in floats(&out.lock().unwrap()).iter().zip([0.1, 0.02, -0.1, 0.0, 0.5, 0.0]).enumerate() {
        assert!((got - exp).abs() < 1e-5, "param {i}: {got} != {exp}");
    }

    // Reenviar el gizmo (p. ej. tras recargar el avatar) vuelve a mandar la pose aunque la cámara no se haya movido.
    out.lock().unwrap().clear();
    service.activate(logging_sink().1).unwrap();
    service.sync_camera_frame(&Pose { pos: [5.0, 1.0, -5.0], rot: [0.0, 90.0, 0.0] }, &frame_sink).unwrap();
    assert_eq!(out.lock().unwrap().len(), 6);
}

#[test]
fn test_18_anchor_yaw_rotates_position_and_rotation_into_rig_axes() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let (_, sink) = logging_sink();
    service.set_anchor(Anchor { eye: [10.0, 1.5, -5.0], yaw: 90.0 }, sink).unwrap();
    let out = Arc::new(Mutex::new(vec![]));
    let frame_sink = msgs_sink(out.clone());
    service.sync_camera_frame(&Pose { pos: [11.0, 1.5, -5.0], rot: [10.0, 120.0, 5.0] }, &frame_sink).unwrap();
    let want = [0.0, 0.0, 1.0 / 50.0, 10.0 / 180.0, 30.0 / 180.0, 5.0 / 180.0];
    for (i, (got, exp)) in floats(&out.lock().unwrap()).iter().zip(want).enumerate() {
        assert!((got - exp).abs() < 1e-5, "param {i}: {got} != {exp}");
    }
}

#[test]
fn test_19_vrchat_pose_echo_drives_the_same_mesh_params_as_the_commanded_pose() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let (_, sink) = logging_sink();
    service.set_anchor(Anchor { eye: [1.0, 1.6, 2.0], yaw: 0.0 }, sink).unwrap();
    let capture = |pose: &Pose| {
        let o = Arc::new(Mutex::new(vec![]));
        service.last_cam_pose.lock().unwrap().take(); // evita el deadband entre capturas
        service.sync_camera_frame(pose, &msgs_sink(o.clone())).unwrap();
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
fn test_20_hide_cam_toggle_sends_bool_and_is_reported() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let (log, sink) = logging_sink();
    service.set_hide_cam(true, &*sink).unwrap();
    assert_eq!(log.lock().unwrap().clone(), vec![(ADDR_IRIS_HIDE_CAM.to_string(), vec![OscType::Bool(true)])]);
    assert!(service.snapshot().hide_cam);
    service.set_hide_cam(false, &*sink).unwrap();
    assert!(!service.snapshot().hide_cam);
}

#[test]
fn test_21_sync_outcomes_are_counted_with_their_skip_reason() {
    let service = VisualizerService::new(local_config());
    let (_, sink) = logging_sink();
    let out = Arc::new(Mutex::new(vec![]));
    let ok_sink = msgs_sink(out.clone());
    let pose = Pose { pos: [1.0, 0.0, 0.0], rot: [0.0; 3] };

    assert_eq!(service.sync_camera_frame(&pose, &ok_sink).unwrap(), SyncOutcome::Disabled);
    service.activate(sink).unwrap();
    assert_eq!(service.sync_camera_frame(&pose, &ok_sink).unwrap(), SyncOutcome::Sent);
    assert_eq!(service.sync_camera_frame(&pose, &ok_sink).unwrap(), SyncOutcome::Deadband);

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
fn test_22_anchor_nudge_shifts_the_whole_rig_frame_and_resets() {
    let service = VisualizerService::new(VisualizerConfig::default());
    let (_, sink) = logging_sink();
    service.set_anchor(Anchor { eye: [0.0; 3], yaw: 0.0 }, sink).unwrap();
    let out = Arc::new(Mutex::new(vec![]));
    let sink_msgs = msgs_sink(out.clone());
    let pose = Pose { pos: [1.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0] };

    service.sync_camera_frame(&pose, &sink_msgs).unwrap();
    assert!((floats(&out.lock().unwrap())[0] - 0.02).abs() < 1e-6);
    service.nudge([0.5, 0.0, 0.0], 0.0);
    assert_eq!(service.resend_last(&sink_msgs).unwrap(), SyncOutcome::Sent);
    assert!((floats(&out.lock().unwrap())[0] - 0.03).abs() < 1e-5);
    service.nudge([0.0; 3], 10.0);
    assert_eq!(service.resend_last(&sink_msgs).unwrap(), SyncOutcome::Sent);
    assert!((floats(&out.lock().unwrap())[4] - 10.0 / 180.0).abs() < 1e-5);
    let s = service.snapshot();
    assert_eq!((s.nudge_pos, s.nudge_yaw), ([0.5, 0.0, 0.0], 10.0));

    service.nudge([100.0, 0.0, 0.0], 0.0);
    assert_eq!(service.snapshot().nudge_pos[0], 5.0, "limitado a ±5 m");
    service.reset_nudge();
    assert_eq!(service.resend_last(&sink_msgs).unwrap(), SyncOutcome::Sent);
    assert!((floats(&out.lock().unwrap())[0] - 0.02).abs() < 1e-6);

    service.nudge([1.0, 0.0, 0.0], 5.0);
    let (_, sink2) = logging_sink();
    service.set_anchor(Anchor { eye: [0.0; 3], yaw: 0.0 }, sink2).unwrap();
    assert_eq!(service.snapshot().nudge_pos, [0.0; 3]);
}

#[test]
fn test_23_avatar_parameter_echoes_from_vrchat_are_recorded() {
    let service = VisualizerService::new(VisualizerConfig::default());
    service.note_avatar_echo("/avatar/parameters/iris_cam_x", 0.25);
    service.note_avatar_echo("/avatar/parameters/iris_cam_x", 0.5);
    service.note_avatar_echo("/avatar/parameters/iris_enable", 1.0);
    service.note_avatar_echo("/avatar/parameters/OtroParametro", 9.0);
    service.note_avatar_echo("/avatar/change", 1.0);
    let mut echoes = service.snapshot().diag.avatar_echo;
    echoes.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(echoes.iter().map(|e| (e.name.as_str(), e.value)).collect::<Vec<_>>(), vec![("iris_cam_x", 0.5), ("iris_enable", 1.0)]);
}

#[test]
fn test_24_avatar_change_resets_the_rig_state_so_it_can_be_shown_again() {
    let service = VisualizerService::new(local_config());
    let (log, sink) = logging_sink();
    service.set_anchor(Anchor { eye: [1.0, 2.0, 3.0], yaw: 40.0 }, sink.clone()).unwrap();
    service.nudge([0.5, 0.0, 0.0], 5.0);
    service.set_hide_cam(true, &*sink).unwrap();
    service.note_avatar_echo("/avatar/parameters/iris_cam_x", 0.3);
    assert!(service.snapshot().enabled);

    service.on_avatar_changed();
    let s = service.snapshot();
    assert!(!s.enabled, "el rig del avatar nuevo está apagado");
    assert_eq!((s.anchor_position, s.nudge_pos, s.nudge_yaw, s.hide_cam), (None, [0.0; 3], 0.0, false));
    assert!(s.diag.avatar_echo.is_empty());

    log.lock().unwrap().clear();
    service.activate(sink).unwrap();
    assert_eq!(log.lock().unwrap()[0].1, vec![OscType::Bool(true)]);
    assert!(service.snapshot().enabled);
}

// ----------------------------------------------------------------------------------------------------------------
// Servicio: path hacia dueño y remotos
// ----------------------------------------------------------------------------------------------------------------

#[test]
fn test_25_queue_sync_path_writes_owner_params_at_once_and_the_bus_afterwards() {
    let service = VisualizerService::new(local_config());
    let (log, sink) = logging_sink();
    service.activate(sink.clone()).unwrap();
    wait_job_done(&log, 1);
    log.lock().unwrap().clear();

    let kfs = [(0.0, [100.0, 2.0, 100.0]), (4.0, [110.0, 3.0, 96.0])];
    let project = project_with(&kfs, 4.0);
    service.queue_sync_path_mesh(&project, None, sink).unwrap();

    // El dueño recibe los parámetros locales de inmediato (antes de que el bus empiece): región + 8·(pc + pr).
    let owner = log.lock().unwrap().clone();
    let owner_n = 3 + 2 * 3 * NET_POINTS;
    assert!(owner.len() >= owner_n);
    assert!(owner[..owner_n].iter().all(|(a, _)| a.contains("iris_reg_") || a.contains("iris_pc_") || a.contains("iris_pr_")));

    // La región se centró alrededor del path (lejos del origen) y la cámara en vivo ahora es relativa a ella.
    let region = service.snapshot().anchor_position.unwrap();
    assert!(region[0] > 90.0 && region[0] < 120.0, "región x = {}", region[0]);
    assert_eq!(region, snap_region(region));

    wait_job_done(&log, 1);
    let l = log.lock().unwrap().clone();
    let slot_writes = l.iter().filter(|(a, v)| a == ADDR_IRIS_NET_SLOT && *v != vec![OscType::Int(0)]).count();
    assert_eq!(slot_writes, 1 + 3 * NET_POINTS);
    assert!(!service.is_syncing_path.load(std::sync::atomic::Ordering::SeqCst));
    assert!(service.last_error.lock().unwrap().is_none());
    assert_eq!(service.snapshot().last_wp_samples.unwrap().len(), NET_POINTS);

    // Sin el rig encendido no hay path que sincronizar.
    let off = VisualizerService::new(local_config());
    assert!(off.queue_sync_path_mesh(&project, None, logging_sink().1).is_err());
}

#[test]
fn test_26_remote_sees_the_same_path_the_owner_sees_even_far_from_the_origin() {
    let service = VisualizerService::new(local_config());
    let (log, sink) = logging_sink();
    service.activate(sink.clone()).unwrap();
    service
        .queue_sync_path_mesh(&project_with(&[(0.0, [213.7, 12.3, -88.1]), (3.0, [230.0, 14.0, -70.5]), (6.0, [205.2, 9.9, -95.0])], 6.0), None, sink)
        .unwrap();
    wait_job_done(&log, 2);

    let mut remote = Remote::new(NetQuant::Round);
    remote.feed_log(&log);
    let path_points = service.snapshot().last_wp_samples.unwrap();
    for (i, p) in path_points.iter().enumerate() {
        assert!(dist(remote.point_pos(i), *p) < 0.002, "punto {i}: {:?} vs {:?}", remote.point_pos(i), p);
    }
}

#[test]
fn test_27_playback_streams_path_t_at_low_rate_and_switches_remotes_to_path_mode() {
    let service = VisualizerService::new(local_config());
    let (log, sink) = logging_sink();
    service.activate(sink.clone()).unwrap();
    let kfs = [(0.0, [0.0, 1.0, 0.0]), (2.0, [14.0, 1.0, 0.0])];
    service.queue_sync_path_mesh(&project_with(&kfs, 2.0), None, sink).unwrap();
    wait_job_done(&log, 2);
    log.lock().unwrap().clear();

    // 2 s de reproducción a 60 Hz comprimidos en ~0.5 s.
    let out = Arc::new(Mutex::new(vec![]));
    let frame_sink = msgs_sink(out);
    let t0 = Instant::now();
    let mut last_pos = [0.0; 3];
    for f in 0..=120 {
        let time = f as f32 / 60.0;
        last_pos = [7.0 * time, 1.0, 0.0];
        service.sync_playback(&Pose { pos: last_pos, rot: [0.0; 3] }, time, last_pos, &frame_sink).unwrap();
        std::thread::sleep(Duration::from_millis(4));
    }
    let elapsed = t0.elapsed();
    wait_until("modo path y T final", || {
        let l = log.lock().unwrap();
        l.iter().any(|(a, v)| a == ADDR_IRIS_NET_SLOT && *v == vec![OscType::Int(slot::MODE_PATH)])
            && l.iter().filter(|(a, _)| a == ADDR_IRIS_PATH_T).last().is_some_and(|(_, v)| float_of(v) > 0.97)
    });
    let l = log.lock().unwrap().clone();
    let t_msgs: Vec<f32> = l.iter().filter(|(a, _)| a == ADDR_IRIS_PATH_T).map(|(_, v)| float_of(v)).collect();
    let max_msgs = (elapsed.as_millis() / 100) as usize + 4;
    assert!(t_msgs.len() >= 3 && t_msgs.len() <= max_msgs, "{} mensajes de path_t en {elapsed:?} (máx. {max_msgs})", t_msgs.len());
    assert!(t_msgs.windows(2).all(|w| w[1] >= w[0] - 0.02), "T crece con la reproducción: {t_msgs:?}");
    assert_eq!(service.snapshot().net_mode, "path");
    let _ = last_pos;

    // El remoto interpola sobre el path: T = 1 es el último punto.
    let mut remote = Remote::new(NetQuant::Round);
    remote.feed_log(&log);
    assert!(remote.path_mode);
}

#[test]
fn test_28_a_hand_moved_camera_is_held_for_remotes_once_it_stops() {
    let service = VisualizerService::new(local_config());
    let (log, sink) = logging_sink();
    service.activate(sink).unwrap();
    wait_job_done(&log, 1);
    log.lock().unwrap().clear();

    let out = Arc::new(Mutex::new(vec![]));
    let frame_sink = msgs_sink(out);
    // Moviéndose: no se manda nada a los remotos.
    for i in 0..5 {
        service.sync_camera_frame(&Pose { pos: [1.0 + i as f32, 1.6, 2.0], rot: [5.0, 20.0, 0.0] }, &frame_sink).unwrap();
        std::thread::sleep(Duration::from_millis(40));
    }
    assert!(log.lock().unwrap().iter().all(|(a, _)| a != ADDR_IRIS_NET_SLOT), "mientras se mueve no hay pose retenida");

    // Quieta 300 ms → pose retenida + modo mano.
    let final_pose = Pose { pos: [5.123, 1.6, 2.0], rot: [5.0, 20.0, 0.0] };
    service.sync_camera_frame(&final_pose, &frame_sink).unwrap();
    wait_job_done(&log, 1);
    let mut remote = Remote::new(NetQuant::Round);
    remote.feed_log(&log);
    assert_eq!(remote.latched, vec![slot::POSE_C, slot::POSE_F, slot::POSE_R, slot::MODE_HAND]);
    assert!(dist(remote.hold_pos(), final_pose.pos) < 0.002, "retenida en {:?}", remote.hold_pos());
    assert!(!remote.path_mode);
    assert_eq!(service.snapshot().net_mode, "hand");
}

#[test]
fn test_29_the_region_recenters_when_the_camera_leaves_the_working_cube() {
    let service = VisualizerService::new(local_config());
    let (log, sink) = logging_sink();
    service.activate(sink).unwrap();
    wait_job_done(&log, 1);
    log.lock().unwrap().clear();

    let out = Arc::new(Mutex::new(vec![]));
    let frame_sink = msgs_sink(out.clone());
    let far = Pose { pos: [300.0, 10.0, -250.0], rot: [0.0; 3] };
    service.sync_camera_frame(&far, &frame_sink).unwrap();

    let region = service.snapshot().anchor_position.unwrap();
    assert_eq!(region, snap_region(far.pos));
    // La pose en vivo ya es relativa al nuevo centro (dentro de ±50 m ⇒ sin saturar).
    assert!(floats(&out.lock().unwrap())[..3].iter().all(|v| v.abs() < 0.2), "{:?}", floats(&out.lock().unwrap()));
    wait_job_done(&log, 1);
    let mut remote = Remote::new(NetQuant::Round);
    remote.feed_log(&log);
    assert_eq!(remote.region_m(), region);
    assert!(log.lock().unwrap().iter().any(|(a, _)| a == ADDR_IRIS_REG_X), "el dueño también recibe la región");

    // Un segundo salto inmediato respeta el enfriamiento (sin ciclos de 15 s encadenados).
    let again = Pose { pos: [-300.0, 10.0, 250.0], rot: [0.0; 3] };
    service.sync_camera_frame(&again, &frame_sink).unwrap();
    assert_eq!(service.snapshot().anchor_position.unwrap(), region);
}

fn slot_writes(log: &Log) -> Vec<i32> {
    log.lock()
        .unwrap()
        .iter()
        .filter(|(a, _)| a == ADDR_IRIS_NET_SLOT)
        .filter_map(|(_, v)| match v[0] {
            OscType::Int(i) if i != 0 => Some(i),
            _ => None,
        })
        .collect()
}

#[test]
fn test_30_urgent_jobs_do_not_wait_for_a_long_path_cycle() {
    let config = VisualizerConfig { mux_timing: MuxTiming { data_ms: 15, slot_ms: 15 }, ..VisualizerConfig::default() };
    let service = VisualizerService::new(config);
    let (log, sink) = logging_sink();
    service.activate(sink.clone()).unwrap();
    wait_job_done(&log, 1);
    log.lock().unwrap().clear();

    service.queue_sync_path_mesh(&project_with(&[(0.0, [0.0; 3]), (3.0, [10.0, 0.0, 0.0])], 3.0), None, sink).unwrap();
    // El ciclo dura ~25 slots × 30 ms; una pose retenida (cámara quieta 300 ms) entra a mitad del ciclo, sin esperar al final.
    let frame_sink = msgs_sink(Arc::new(Mutex::new(vec![])));
    service.sync_camera_frame(&Pose { pos: [1.0, 1.0, 1.0], rot: [0.0; 3] }, &frame_sink).unwrap();
    wait_job_done(&log, 2);

    let slots = slot_writes(&log);
    let hold_at = slots.iter().position(|s| *s == slot::POSE_C).expect("la pose retenida se envió");
    let last_path = slots.iter().rposition(|s| *s == slot::pos_f(NET_POINTS - 1)).expect("el ciclo del path terminó");
    assert!(hold_at < last_path, "la pose retenida debe colarse dentro del ciclo del path ({hold_at} vs {last_path})");
}

#[test]
fn test_31_a_newer_path_replaces_the_one_being_sent() {
    let config = VisualizerConfig { mux_timing: MuxTiming { data_ms: 10, slot_ms: 10 }, ..VisualizerConfig::default() };
    let service = VisualizerService::new(config);
    let (log, sink) = logging_sink();
    service.activate(sink.clone()).unwrap();
    wait_job_done(&log, 1);
    log.lock().unwrap().clear();

    let a = project_with(&[(0.0, [0.0; 3]), (3.0, [10.0, 0.0, 0.0])], 3.0);
    let b = project_with(&[(0.0, [0.0; 3]), (3.0, [0.0, 0.0, 20.0])], 3.0);
    service.queue_sync_path_mesh(&a, None, sink.clone()).unwrap();
    std::thread::sleep(Duration::from_millis(120));
    service.queue_sync_path_mesh(&b, None, sink).unwrap();
    wait_job_done(&log, 2);
    // Dar tiempo a que el segundo ciclo termine: su último mensaje también es `iris_net_slot = 0`.
    wait_until("segundo ciclo completo", || slot_writes(&log).iter().filter(|s| **s == slot::pos_f(NET_POINTS - 1)).count() >= 1);

    let slots = slot_writes(&log);
    let starts = slots.iter().filter(|s| **s == slot::REGION).count();
    let ends = slots.iter().filter(|s| **s == slot::pos_f(NET_POINTS - 1)).count();
    assert_eq!((starts, ends), (2, 1), "el path viejo se aborta y solo el nuevo llega al final: {slots:?}");
}
