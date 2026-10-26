//! Estado de la cámara (fuente de verdad) alimentado por los ecos de VRChat.
use crate::osc::protocol::{kind, Kind, Pose, Value};
use crate::osc::Osc;
use rosc::{OscMessage, OscType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    Opened,
    Closed,
}

/// Eventos estructurados recibidos por OSC desde VRChat para alimentar al gestor de anclaje (`AnchorManager`).
#[derive(Clone, Debug, PartialEq)]
pub enum CameraOscEvent {
    Mode {
        prev_mode: u8,
        new_mode: u8,
        timestamp_ms: u64,
    },
    Pose {
        pose: Pose,
        seq: u64,
        timestamp_ms: u64,
    },
    LookAtMe {
        enabled: bool,
        offsets_xy: Option<[f32; 2]>,
        timestamp_ms: u64,
    },
}

/// Instantánea verificada del estado de cámara para captura de puntos y diagnóstico UI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraSnapshot {
    pub open: bool,
    /// Modo actual reportado por `/usercamera/Mode` (`0` = Off, `1` = Photo, `2` = Stream, `3..=6` = otros).
    #[serde(default)]
    pub mode: u8,
    /// `true` si el puerto UDP 9001 está recibiendo ecos; `false` si opera en modo Control Abierto.
    #[serde(default = "default_true")]
    pub rx_available: bool,
    pub pose: Option<Pose>,
    /// `true` si llegó al menos una Pose desde la última vez que se abrió la cámara.
    pub pose_fresh: bool,
    /// Contador monótono de paquetes `/usercamera/Pose` recibidos.
    pub pose_seq: u64,
    /// Timestamp (ms epoch) del último paquete `Pose` recibido de VRChat.
    pub pose_epoch_ms: Option<u64>,
    pub zoom: Option<f32>,
    pub exposure: Option<f32>,
    pub focal_distance: Option<f32>,
    pub aperture: Option<f32>,
    pub look_at_me: Option<bool>,
    pub smooth_movement: Option<bool>,
    pub streaming: Option<bool>,
}

impl Default for CameraSnapshot {
    fn default() -> Self {
        CameraSnapshot {
            open: false,
            mode: 0,
            rx_available: true,
            pose: None,
            pose_fresh: false,
            pose_seq: 0,
            pose_epoch_ms: None,
            zoom: None,
            exposure: None,
            focal_distance: None,
            aperture: None,
            look_at_me: None,
            smooth_movement: None,
            streaming: None,
        }
    }
}

fn default_true() -> bool {
    true
}

pub struct State {
    pub open: bool,
    pub mode: u8,
    pub rx_available: bool,
    pub pose: Option<Pose>,
    pub pose_fresh: bool,
    pub pose_seq: u64,
    pub pose_epoch_ms: Option<u64>,
    /// Último valor visto por endpoint (eco de VRChat).
    pub values: HashMap<String, Value>,
    /// Último valor enviado por endpoint para deduplicar tráfico óptico redundante a 60 Hz.
    pub last_sent: HashMap<String, Value>,
    /// Últimos valores enviados por endpoint (ventana corta). Permite distinguir el eco (tardío o duplicado: VRChat
    /// ecoa Zoom/Exposure dos veces y se observaron retrasos de varios segundos) de un cambio hecho por el usuario.
    pub recent_sent: HashMap<String, std::collections::VecDeque<Value>>,
    /// Instante del último envío por slider de `PACED` (ver `should_send_paced`).
    pub paced_at: HashMap<String, Instant>,
}

/// Sliders que VRChat encola y consume a 1 mensaje por frame (feedback.vrchat.com "OSC /usercamera/Zoom processes
/// messages slowly"): enviarlos más rápido que el framerate de VRChat acumula backlog y el valor sigue moviéndose
/// después de terminar el recorrido. `/usercamera/Pose` no sufre esto y no se limita.
pub const PACED: [&str; 4] = ["Zoom", "Exposure", "FocalDistance", "Aperture"];
/// Hueco mínimo entre dos mensajes de un slider de `PACED` (~30 Hz; 31 ms en vez de 33,3 para tolerar el jitter del
/// ticker de 60 Hz sin caer a 20 Hz). ponytail: cap fijo; si VRChat corre por debajo de 30 fps aún habría algo de
/// backlog; la mejora sería esperar el eco del valor anterior.
pub const PACED_MIN_GAP: std::time::Duration = std::time::Duration::from_millis(31);

/// Cuántos valores enviados recientes se recuerdan por endpoint para reconocer ecos propios.
const RECENT_SENT_WINDOW: usize = 512;

impl Default for State {
    fn default() -> Self {
        State {
            open: false,
            mode: 0,
            rx_available: true,
            pose: None,
            pose_fresh: false,
            pose_seq: 0,
            pose_epoch_ms: None,
            values: HashMap::new(),
            last_sent: HashMap::new(),
            recent_sent: HashMap::new(),
            paced_at: HashMap::new(),
        }
    }
}

fn values_close_enough(a: Value, b: Value) -> bool {
    match (a, b) {
        (Value::F(x), Value::F(y)) => (x - y).abs() < 1e-3,
        (Value::I(x), Value::I(y)) => x == y,
        (Value::B(x), Value::B(y)) => x == y,
        _ => false,
    }
}

pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl State {
    /// `Mode 0` = cerrada, `Mode ≠ 0` = abierta; un `Pose` también implica abierta
    /// (VRChat no lo emite con la cámara cerrada). Devuelve el cambio abierta/cerrada, si lo hubo.
    pub fn apply(&mut self, m: &OscMessage) -> Option<Change> {
        self.apply_with_event(m, now_epoch_ms()).0
    }

    /// Aplica un mensaje OSC con marca temporal explícita y devuelve tanto el cambio `Opened`/`Closed`
    /// como el evento tipado `CameraOscEvent` para el gestor de anclaje (`AnchorManager`).
    pub fn apply_with_event(
        &mut self,
        m: &OscMessage,
        now_ms: u64,
    ) -> (Option<Change>, Option<CameraOscEvent>) {
        let Some(name) = m.addr.strip_prefix("/usercamera/") else {
            return (None, None);
        };
        if name == "Pose" {
            let Some(parsed_pose) = Pose::from_args(&m.args) else {
                return (None, None);
            };
            self.pose = Some(parsed_pose);
            self.pose_fresh = true;
            self.pose_seq = self.pose_seq.wrapping_add(1);
            self.pose_epoch_ms = Some(now_ms);
            if self.mode == 0 {
                self.mode = 1;
            }
            let was = std::mem::replace(&mut self.open, true);
            let change = if !was {
                self.last_sent.clear();
                Some(Change::Opened)
            } else {
                None
            };
            let ev = CameraOscEvent::Pose {
                pose: parsed_pose,
                seq: self.pose_seq,
                timestamp_ms: now_ms,
            };
            return (change, Some(ev));
        }

        let Some(k) = kind(name) else {
            return (None, None);
        };
        let Some(first_arg) = m.args.first() else {
            return (None, None);
        };
        let v = match (k, first_arg) {
            (Kind::Float(..), OscType::Float(f)) if f.is_finite() => Value::F(*f),
            (Kind::Int(..), OscType::Int(i)) => Value::I(*i),
            (Kind::Bool | Kind::Trigger, OscType::Bool(b)) => Value::B(*b),
            _ => return (None, None),
        };
        self.values.insert(name.to_string(), v);
        // Un eco de un valor que nosotros enviamos NO debe tocar `last_sent` (un eco tardío con un valor viejo haría que
        // `should_send` omitiera el siguiente envío y el slider se quedara atascado). Solo un valor ajeno (el usuario
        // movió el slider en VRChat) actualiza `last_sent`, para poder sobrescribirlo después.
        let is_own_echo = self
            .recent_sent
            .get(name)
            .is_some_and(|q| q.iter().any(|s| values_close_enough(*s, v)));
        if !is_own_echo {
            self.last_sent.insert(name.to_string(), v);
        }

        if matches!(name, "LookAtMe" | "LookAtMeXOffset" | "LookAtMeYOffset") {
            let enabled = match self.values.get("LookAtMe") {
                Some(Value::B(b)) => *b,
                _ => false,
            };
            let x_off = match self.values.get("LookAtMeXOffset") {
                Some(Value::F(f)) => *f,
                _ => 0.0,
            };
            let y_off = match self.values.get("LookAtMeYOffset") {
                Some(Value::F(f)) => *f,
                _ => 0.0,
            };
            let ev = CameraOscEvent::LookAtMe {
                enabled,
                offsets_xy: Some([x_off, y_off]),
                timestamp_ms: now_ms,
            };
            return (None, Some(ev));
        }

        if name != "Mode" {
            return (None, None);
        }

        let Value::I(raw_mode) = v else {
            return (None, None);
        };
        let prev_mode = self.mode;
        let new_mode = raw_mode.clamp(0, 6) as u8;
        self.mode = new_mode;
        let is_open = new_mode != 0;
        if !is_open {
            // Al cerrarse la cámara, la pose anterior deja de ser fresca y limpiamos caché de envíos.
            self.pose_fresh = false;
            self.last_sent.clear();
        }
        let was = std::mem::replace(&mut self.open, is_open);
        let change = match (was, is_open) {
            (false, true) => {
                self.last_sent.clear();
                Some(Change::Opened)
            }
            (true, false) => Some(Change::Closed),
            _ => None,
        };
        let ev = CameraOscEvent::Mode {
            prev_mode,
            new_mode,
            timestamp_ms: now_ms,
        };
        (change, Some(ev))
    }

    pub fn snapshot(&self) -> CameraSnapshot {
        let get_f = |k: &str| match self.values.get(k).or_else(|| self.last_sent.get(k)) {
            Some(Value::F(v)) => Some(*v),
            _ => None,
        };
        let get_b = |k: &str| match self.values.get(k).or_else(|| self.last_sent.get(k)) {
            Some(Value::B(v)) => Some(*v),
            _ => None,
        };
        CameraSnapshot {
            open: self.open,
            mode: self.mode,
            rx_available: self.rx_available,
            pose: self.pose,
            pose_fresh: self.pose_fresh,
            pose_seq: self.pose_seq,
            pose_epoch_ms: self.pose_epoch_ms,
            zoom: get_f("Zoom"),
            exposure: get_f("Exposure"),
            focal_distance: get_f("FocalDistance"),
            aperture: get_f("Aperture"),
            look_at_me: get_b("LookAtMe"),
            smooth_movement: get_b("SmoothMovement"),
            streaming: get_b("Streaming"),
        }
    }

    /// `Mode` y `Pose` con la cámara cerrada no hacen nada, pero `Mode` sí devuelve eco: si la app lo
    /// enviara, el estado creería que está abierta. Por eso se bloquean aquí (salvo cuando `rx_available` es `false`
    /// y operamos en modo de Control Abierto).
    fn check_writable(&self, name: &str) -> Result<(), String> {
        if self.rx_available && !self.open && matches!(name, "Mode" | "Pose") {
            return Err("cámara cerrada: ábrela en VRChat".into());
        }
        Ok(())
    }

    /// Determina si debemos enviar el valor por UDP o si es idéntico al último enviado (evita inundar a 60 Hz).
    pub fn should_send(&mut self, name: &str, value: Value) -> Result<bool, String> {
        self.should_send_inner(name, value, false)
    }

    /// `exact`: solo omite el envío si el valor es idéntico al último (sin la tolerancia de 1e-3 del dedupe).
    fn should_send_inner(&mut self, name: &str, value: Value, exact: bool) -> Result<bool, String> {
        self.check_writable(name)?;
        if matches!(kind(name), Some(Kind::Trigger)) {
            return Ok(true);
        }
        let same = |prev: Value| if exact { prev == value } else { values_close_enough(prev, value) };
        if self.last_sent.get(name).copied().is_some_and(same) {
            return Ok(false);
        }
        self.last_sent.insert(name.to_string(), value);
        let q = self.recent_sent.entry(name.to_string()).or_default();
        q.push_back(value);
        if q.len() > RECENT_SENT_WINDOW {
            q.pop_front();
        }
        Ok(true)
    }

    /// Como `should_send`, pero con pacing para los sliders que VRChat encola (ver `PACED`). `flush` = último valor
    /// de un recorrido: se envía sin esperar.
    pub fn should_send_paced(&mut self, name: &str, value: Value, now: Instant, flush: bool) -> Result<bool, String> {
        let paced = PACED.contains(&name);
        if paced && !flush && self.paced_at.get(name).is_some_and(|t| now.saturating_duration_since(*t) < PACED_MIN_GAP) {
            return Ok(false); // sin tocar `last_sent`: el siguiente tick fuera del hueco enviará el valor más reciente
        }
        // El paquete de cierre lleva el destino exacto: VRChat termina en el valor final, no a 1e-3 de él.
        let send = self.should_send_inner(name, value, flush)?;
        if paced && send {
            self.paced_at.insert(name.to_string(), now);
        }
        Ok(send)
    }
}

/// Valores por defecto de VRChat (tabla de CLAUDE.md; Lightness 50 = máximo medido). No incluye `Mode`,
/// `Lock` ni otras preferencias del usuario.
pub const DEFAULTS: &[(&str, Value)] = &[
    ("Zoom", Value::F(45.0)),
    ("Exposure", Value::F(0.0)),
    ("FocalDistance", Value::F(1.5)),
    ("Aperture", Value::F(15.0)),
    ("Hue", Value::F(120.0)),
    ("Saturation", Value::F(100.0)),
    ("Lightness", Value::F(50.0)),
    ("LookAtMeXOffset", Value::F(0.0)),
    ("LookAtMeYOffset", Value::F(0.0)),
    // En VRChat (/usercamera/FlySpeed, ver docs/oscquery-usercamera.json), 0.2 corresponde al factor
    // por defecto 3.0 en la UI (3.0 / 15.0 = 0.2); enviar 3.0 satura el slider en 15.0.
    ("FlySpeed", Value::F(0.2)),
    ("TurnSpeed", Value::F(1.0)),
    ("SmoothingStrength", Value::F(5.0)),
    ("PhotoRate", Value::F(1.0)),
    ("Duration", Value::F(2.0)),
    ("LookAtMe", Value::B(false)),
    ("AutoLevelRoll", Value::B(false)),
    ("AutoLevelPitch", Value::B(false)),
    ("SmoothMovement", Value::B(false)),
    ("GreenScreen", Value::B(false)),
    ("Streaming", Value::B(false)),
    ("CameraEars", Value::B(false)),
    ("ShowFocus", Value::B(false)),
    ("ShowUIInCamera", Value::B(false)),
    ("LocalPlayer", Value::B(true)),
    ("RemotePlayer", Value::B(true)),
    ("Environment", Value::B(true)),
    ("OrientationIsLandscape", Value::B(true)),
];

pub struct Camera {
    osc: Osc,
    pub state: Arc<Mutex<State>>,
}

impl Camera {
    /// `on_change` corre en el hilo de recepción: úsalo para reaplicar el preset al abrirse.
    pub fn start(target: SocketAddr, on_change: impl Fn(Change) + Send + 'static) -> Result<Camera, String> {
        Self::start_with_events(target, move |change, _ev| {
            if let Some(c) = change {
                on_change(c);
            }
        })
    }

    /// Inicia el cliente y escucha OSC invocando `on_event(change, osc_event)` por cada mensaje relevante.
    pub fn start_with_events(
        target: SocketAddr,
        on_event: impl Fn(Option<Change>, Option<CameraOscEvent>) + Send + 'static,
    ) -> Result<Camera, String> {
        let state = Arc::new(Mutex::new(State::default()));
        let s = state.clone();
        let osc = Osc::start(target, move |m| {
            let now_ms = now_epoch_ms();
            let (change, ev) = s.lock().unwrap().apply_with_event(&m, now_ms);
            if change.is_some() || ev.is_some() {
                on_event(change, ev);
            }
        })?;
        {
            let mut st = state.lock().unwrap();
            st.rx_available = osc.rx_bound;
        }
        Ok(Camera { osc, state })
    }

    /// Inicia el cliente y escucha OSC enrutando mensajes `/avatar/*` al módulo de avatar
    /// y mensajes `/usercamera/*` al pipeline de cámara y anclaje.
    pub fn start_with_avatar_router(
        target: SocketAddr,
        on_event: impl Fn(Option<Change>, Option<CameraOscEvent>) + Send + 'static,
        mut on_avatar_msg: impl FnMut(OscMessage) + Send + 'static,
    ) -> Result<Camera, String> {
        let state = Arc::new(Mutex::new(State::default()));
        let s = state.clone();
        let osc = Osc::start(target, move |m| {
            if m.addr.starts_with("/avatar/") {
                on_avatar_msg(m);
                return;
            }
            let now_ms = now_epoch_ms();
            let (change, ev) = s.lock().unwrap().apply_with_event(&m, now_ms);
            if change.is_some() || ev.is_some() {
                on_event(change, ev);
            }
        })?;
        {
            let mut st = state.lock().unwrap();
            st.rx_available = osc.rx_bound;
        }
        Ok(Camera { osc, state })
    }

    /// Indica si el socket UDP de escucha en 9001 está enlazado activamente.
    pub fn is_rx_bound(&self) -> bool {
        self.osc.rx_bound
    }

    pub fn send_raw(&self, addr: String, args: Vec<rosc::OscType>) -> Result<(), String> {
        self.osc.send_raw(addr, args)
    }

    /// Envía un paquete OSC Bundle atómicamente a VRChat.
    pub fn send_bundle(&self, messages: Vec<rosc::OscMessage>) -> Result<(), String> {
        self.osc.send_bundle(messages)
    }

    pub fn set(&self, name: &str, value: Value) -> Result<(), String> {
        if !self.state.lock().unwrap().should_send(name, value)? {
            return Ok(());
        }
        self.osc.set(name, value)
    }

    /// `set` con pacing para Zoom/Exposure/FocalDistance/Aperture (VRChat procesa 1 mensaje por frame).
    pub fn set_paced(&self, name: &str, value: Value, flush: bool) -> Result<(), String> {
        if !self.state.lock().unwrap().should_send_paced(name, value, Instant::now(), flush)? {
            return Ok(());
        }
        self.osc.set(name, value)
    }

    pub fn pose(&self, pose: &Pose) -> Result<(), String> {
        self.state.lock().unwrap().check_writable("Pose")?;
        self.osc.pose(pose)
    }

    pub fn reset(&self) -> Result<(), String> {
        self.state.lock().unwrap().last_sent.clear();
        DEFAULTS.iter().try_for_each(|(n, v)| self.set(n, *v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(name: &str, args: Vec<OscType>) -> OscMessage {
        OscMessage { addr: format!("/usercamera/{name}"), args }
    }

    #[test]
    fn tracks_open_close_and_values() {
        let mut s = State::default();
        assert_eq!(s.apply(&msg("Mode", vec![OscType::Int(0)])), None);
        assert_eq!(s.apply(&msg("Mode", vec![OscType::Int(1)])), Some(Change::Opened));
        assert_eq!(s.apply(&msg("Mode", vec![OscType::Int(2)])), None);
        assert_eq!(s.apply(&msg("Zoom", vec![OscType::Float(30.0)])), None);
        assert_eq!(s.values["Zoom"], Value::F(30.0));
        assert_eq!(s.apply(&msg("Mode", vec![OscType::Int(0)])), Some(Change::Closed));
    }

    #[test]
    fn pose_implies_open() {
        let mut s = State::default();
        let pose: Vec<_> = [1.0, 1.5, 2.0, 10.0, 90.0, 0.0].iter().map(|v| OscType::Float(*v)).collect();
        assert_eq!(s.apply(&msg("Pose", pose)), Some(Change::Opened));
        assert_eq!(s.pose.unwrap().pos, [1.0, 1.5, 2.0]);
        // mensajes ajenos o malformados se ignoran
        assert_eq!(s.apply(&msg("Pose", vec![OscType::Float(1.0)])), None);
        assert_eq!(s.apply(&msg("Zoom", vec![OscType::Bool(true)])), None);
        assert_eq!(s.apply(&OscMessage { addr: "/avatar/x".into(), args: vec![] }), None);
    }

    #[test]
    fn defaults_are_valid_and_in_range() {
        for (n, v) in DEFAULTS {
            assert!(crate::osc::protocol::encode(n, *v).is_ok(), "{n}");
            if let Value::F(f) = v {
                assert!(crate::osc::protocol::in_range(n, *f), "{n}={f} fuera de rango");
            }
        }
    }

    #[test]
    fn blocks_mode_and_pose_while_closed() {
        let mut s = State::default();
        assert!(s.check_writable("Mode").is_err());
        assert!(s.check_writable("Pose").is_err());
        assert!(s.check_writable("Zoom").is_ok());
        s.open = true;
        assert!(s.check_writable("Mode").is_ok());
    }

    #[test]
    fn deduplicates_identical_slider_writes_and_allows_triggers() {
        let mut s = State::default();
        s.open = true;
        assert!(s.should_send("Zoom", Value::F(45.0)).unwrap());
        assert!(!s.should_send("Zoom", Value::F(45.0002)).unwrap());
        assert!(s.should_send("Zoom", Value::F(46.0)).unwrap());
        // Los triggers (Capture) siempre se envían
        assert!(s.should_send("Capture", Value::B(true)).unwrap());
        assert!(s.should_send("Capture", Value::B(true)).unwrap());
    }

    #[test]
    fn invalidates_pose_freshness_on_close_until_new_pose() {
        let mut s = State::default();
        assert!(!s.snapshot().pose_fresh);
        let pose: Vec<_> = [1.0, 1.5, 2.0, 10.0, 90.0, 0.0].iter().map(|v| OscType::Float(*v)).collect();
        s.apply(&msg("Pose", pose.clone()));
        assert!(s.snapshot().pose_fresh);
        assert_eq!(s.snapshot().pose_seq, 1);
        // Cerrar cámara invalida la frescura de la pose previa
        s.apply(&msg("Mode", vec![OscType::Int(0)]));
        assert!(!s.snapshot().pose_fresh);
        // Reabrir por Mode no hace fresca la pose antigua hasta recibir un nuevo paquete Pose
        s.apply(&msg("Mode", vec![OscType::Int(1)]));
        assert!(!s.snapshot().pose_fresh);
        s.apply(&msg("Pose", pose));
        assert!(s.snapshot().pose_fresh);
        assert_eq!(s.snapshot().pose_seq, 2);
    }

    #[test]
    fn stale_own_echo_does_not_suppress_next_send() {
        let mut s = State::default();
        s.open = true;
        // Enviamos 50 → 60 → 50; llega tarde el eco de 60 y de 50 (con retraso).
        assert!(s.should_send("Zoom", Value::F(50.0)).unwrap());
        assert!(s.should_send("Zoom", Value::F(60.0)).unwrap());
        s.apply(&msg("Zoom", vec![OscType::Float(50.0)]));
        // Antes: el eco viejo (50) sobrescribía last_sent y el 50 siguiente se omitía con VRChat aún en 60.
        s.apply(&msg("Zoom", vec![OscType::Float(60.0)]));
        assert!(s.should_send("Zoom", Value::F(50.0)).unwrap(), "debe reenviar 50: last_sent era 60");
    }

    #[test]
    fn user_slider_change_in_vrchat_can_be_overwritten() {
        let mut s = State::default();
        s.open = true;
        assert!(s.should_send("Zoom", Value::F(50.0)).unwrap());
        // El usuario mueve el slider en VRChat a 90 (valor que nunca enviamos).
        s.apply(&msg("Zoom", vec![OscType::Float(90.0)]));
        assert!(s.should_send("Zoom", Value::F(50.0)).unwrap(), "debe poder restaurar 50");
    }

    #[test]
    fn snapshot_tracks_boolean_toggles() {
        let mut s = State::default();
        assert_eq!(s.snapshot().look_at_me, None);
        assert_eq!(s.snapshot().streaming, None);
        s.apply(&msg("LookAtMe", vec![OscType::Bool(true)]));
        s.apply(&msg("Streaming", vec![OscType::Bool(false)]));
        s.apply(&msg("SmoothMovement", vec![OscType::Bool(true)]));
        assert_eq!(s.snapshot().look_at_me, Some(true));
        assert_eq!(s.snapshot().streaming, Some(false));
        assert_eq!(s.snapshot().smooth_movement, Some(true));
    }

    #[test]
    fn emits_structured_camera_osc_events_for_mode_pose_and_look_at_me() {
        let mut s = State::default();
        let (ch1, ev1) = s.apply_with_event(&msg("Mode", vec![OscType::Int(2)]), 1000);
        assert_eq!(ch1, Some(Change::Opened));
        assert_eq!(
            ev1,
            Some(CameraOscEvent::Mode {
                prev_mode: 0,
                new_mode: 2,
                timestamp_ms: 1000
            })
        );
        assert_eq!(s.snapshot().mode, 2);

        let (ch2, ev2) = s.apply_with_event(&msg("LookAtMe", vec![OscType::Bool(true)]), 1010);
        assert_eq!(ch2, None);
        assert_eq!(
            ev2,
            Some(CameraOscEvent::LookAtMe {
                enabled: true,
                offsets_xy: Some([0.0, 0.0]),
                timestamp_ms: 1010
            })
        );
    }

    use crate::studio::model::Transition;
    use std::time::Duration;

    /// Simula un recorrido `secs` de Zoom 45→120 con ticks a `hz`, pasando cada tick por el gate. El último tick
    /// (t = T) lleva `flush`. Devuelve (instante, valor) de cada Zoom que realmente saldría por UDP.
    fn simulate_zoom_ramp(secs: f32, hz: u32, curve: Transition) -> Vec<(Duration, f32)> {
        let mut s = State::default();
        s.open = true;
        let base = Instant::now();
        let ticks = (secs * hz as f32).round() as u32;
        let mut sent = vec![];
        for i in 0..=ticks {
            let t = Duration::from_secs_f64(i as f64 / hz as f64);
            let z = 45.0 + 75.0 * curve.apply(i as f32 / ticks as f32);
            if s.should_send_paced("Zoom", Value::F(z), base + t, i == ticks).unwrap() {
                sent.push((t, z));
            }
        }
        sent
    }

    #[test]
    fn zoom_is_rate_limited_and_final_value_is_exact_for_any_duration_curve_and_tick_rate() {
        for secs in [1.0_f32, 2.5, 10.0] {
            for curve in [Transition::Linear, Transition::SmoothInOut] {
                for hz in [60_u32, 90, 144] {
                    let sent = simulate_zoom_ramp(secs, hz, curve);
                    let ctx = format!("{secs}s {curve:?} {hz}Hz");
                    for w in sent.windows(2) {
                        // El paquete de cierre (flush) puede llegar antes del hueco mínimo; el resto no.
                        if w[1].0 < Duration::from_secs_f32(secs) {
                            assert!(w[1].0 - w[0].0 >= PACED_MIN_GAP, "{ctx}: hueco {:?}", w[1].0 - w[0].0);
                        }
                    }
                    for (i, (t0, _)) in sent.iter().enumerate() {
                        let in_window = sent[i..].iter().filter(|(t, _)| *t - *t0 < Duration::from_secs(1)).count();
                        assert!(in_window <= 33, "{ctx}: {in_window} Zoom en 1 s");
                    }
                    let (t_last, z_last) = *sent.last().unwrap();
                    assert_eq!(z_last, 120.0, "{ctx}: valor final exacto");
                    assert_eq!(t_last, Duration::from_secs_f32(secs), "{ctx}: el cierre sale en t = T");
                }
            }
        }
    }

    #[test]
    fn pacing_only_applies_to_queued_sliders_and_flush_bypasses_it() {
        let mut s = State::default();
        s.open = true;
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(5);
        assert!(s.should_send_paced("Zoom", Value::F(50.0), t0, false).unwrap());
        assert!(!s.should_send_paced("Zoom", Value::F(51.0), t1, false).unwrap(), "dentro del hueco mínimo");
        assert!(s.should_send_paced("Zoom", Value::F(52.0), t1, true).unwrap(), "flush ignora el hueco");
        // Otros sliders no se frenan
        assert!(s.should_send_paced("Hue", Value::F(10.0), t0, false).unwrap());
        assert!(s.should_send_paced("Hue", Value::F(20.0), t1, false).unwrap());
        // Cada slider con pacing lleva su propio reloj
        assert!(s.should_send_paced("Exposure", Value::F(1.0), t1, false).unwrap());
    }
}
