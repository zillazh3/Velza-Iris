//! Bus de logs en memoria para la vista «Logs»: un anillo de entradas más la tasa de mensajes OSC.
//!
//! Es global (no hay que pasar un `AppHandle` por `Osc`/`Camera`) y la UI lo consulta por polling con un cursor,
//! igual que el resto del backend. Cada entrada se formatea en el momento de registrarla; el coste es de unos µs,
//! despreciable a los ~1000 msg/s del peor caso (Studio reproduciendo con el visualizador encendido).
use rosc::{OscMessage, OscPacket, OscType};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// ≈ 4 s del peor caso; los eventos de Engine/Error comparten anillo pero la UI los guarda aparte.
pub const RING_CAP: usize = 4096;
/// Máximo de entradas por consulta: acota el tamaño de cada respuesta IPC.
const MAX_BATCH: usize = 2000;
const MAX_PAYLOAD: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogKind {
    OscIn,
    OscOut,
    Engine,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub seq: u64,
    /// Milisegundos desde epoch (la UI los formatea en hora local).
    pub t_ms: u64,
    pub kind: LogKind,
    pub addr: String,
    pub payload: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogBatch {
    pub entries: Vec<LogEntry>,
    /// Cursor para la siguiente consulta (`seq` de la última entrada vista).
    pub next_cursor: u64,
    /// Entradas que se perdieron entre `cursor` y la primera devuelta (el anillo rodó o el lote se recortó).
    pub dropped: u64,
    pub rate_in: u32,
    pub rate_out: u32,
}

struct Rate {
    window_start: Instant,
    count: u32,
    last: u32,
}

impl Rate {
    fn new() -> Self {
        Rate { window_start: Instant::now(), count: 0, last: 0 }
    }

    fn roll(&mut self, now: Instant) {
        let elapsed = now.duration_since(self.window_start).as_millis();
        if elapsed >= 1000 {
            // Si pasó más de una ventana entera sin tráfico, la tasa anterior ya no es válida.
            self.last = if elapsed < 2000 { self.count } else { 0 };
            self.count = 0;
            self.window_start = now;
        }
    }

    fn hit(&mut self, now: Instant) {
        self.roll(now);
        self.count += 1;
    }

    fn per_sec(&mut self, now: Instant) -> u32 {
        self.roll(now);
        self.last
    }
}

pub struct LogBus {
    next_seq: u64,
    ring: VecDeque<LogEntry>,
    rate_in: Rate,
    rate_out: Rate,
    /// Último valor visto por clave de transición (ver [`edge`]).
    edges: HashMap<&'static str, bool>,
}

impl LogBus {
    pub fn new() -> Self {
        LogBus {
            next_seq: 1,
            ring: VecDeque::with_capacity(RING_CAP),
            rate_in: Rate::new(),
            rate_out: Rate::new(),
            edges: HashMap::new(),
        }
    }

    fn push(&mut self, kind: LogKind, addr: String, mut payload: String) {
        let now = Instant::now();
        match kind {
            LogKind::OscIn => self.rate_in.hit(now),
            LogKind::OscOut => self.rate_out.hit(now),
            _ => {}
        }
        if payload.len() > MAX_PAYLOAD {
            let cut = (0..=MAX_PAYLOAD).rev().find(|i| payload.is_char_boundary(*i)).unwrap_or(0);
            payload.truncate(cut);
            payload.push('…');
        }
        let t_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        if self.ring.len() >= RING_CAP {
            self.ring.pop_front();
        }
        self.ring.push_back(LogEntry { seq: self.next_seq, t_ms, kind, addr, payload });
        self.next_seq += 1;
    }

    fn edge(&mut self, key: &'static str, value: bool, tag: &str, on: &str, off: &str) {
        let prev = self.edges.insert(key, value);
        if prev.unwrap_or(false) != value {
            self.push(LogKind::Engine, tag.to_owned(), if value { on } else { off }.to_owned());
        }
    }

    pub fn since(&mut self, cursor: u64) -> LogBatch {
        let now = Instant::now();
        let last_seq = self.next_seq - 1;
        // Las entradas con seq > cursor son las últimas (`last_seq - cursor`) del anillo.
        let available = (last_seq.saturating_sub(cursor) as usize).min(self.ring.len());
        let take = available.min(MAX_BATCH);
        let skip = self.ring.len() - take;
        let entries: Vec<LogEntry> = self.ring.iter().skip(skip).cloned().collect();
        let first_seen = entries.first().map(|e| e.seq).unwrap_or(last_seq + 1);
        LogBatch {
            entries,
            next_cursor: last_seq,
            dropped: first_seen.saturating_sub(cursor + 1),
            rate_in: self.rate_in.per_sec(now),
            rate_out: self.rate_out.per_sec(now),
        }
    }
}

impl Default for LogBus {
    fn default() -> Self {
        Self::new()
    }
}

fn bus() -> &'static Mutex<LogBus> {
    static BUS: OnceLock<Mutex<LogBus>> = OnceLock::new();
    BUS.get_or_init(|| Mutex::new(LogBus::new()))
}

fn with(f: impl FnOnce(&mut LogBus)) {
    // Un panic en otro hilo no debe dejar sin logs al resto: se ignora el envenenamiento.
    let mut g = bus().lock().unwrap_or_else(|p| p.into_inner());
    f(&mut g);
}

fn fmt_arg(a: &OscType) -> String {
    match a {
        OscType::Float(v) => format!("{v:.3}"),
        OscType::Double(v) => format!("{v:.3}"),
        OscType::Int(v) => v.to_string(),
        OscType::Long(v) => v.to_string(),
        OscType::Bool(v) => v.to_string(),
        OscType::String(s) => format!("{s:?}"),
        other => format!("{other:?}"),
    }
}

fn fmt_args(args: &[OscType]) -> String {
    match args {
        [one] => fmt_arg(one),
        many => format!("[{}]", many.iter().map(fmt_arg).collect::<Vec<_>>().join(", ")),
    }
}

pub fn osc_in(msg: &OscMessage) {
    with(|b| b.push(LogKind::OscIn, msg.addr.clone(), fmt_args(&msg.args)));
}

pub fn osc_out(addr: &str, args: &[OscType]) {
    with(|b| b.push(LogKind::OscOut, addr.to_owned(), fmt_args(args)));
}

/// Registra cada mensaje de un paquete enviado (los bundles se aplanan sin clonarlos).
pub fn osc_out_packet(p: &OscPacket) {
    match p {
        OscPacket::Message(m) => osc_out(&m.addr, &m.args),
        OscPacket::Bundle(b) => b.content.iter().for_each(osc_out_packet),
    }
}

/// Evento del motor (conexión, ancla, reproducción…). `tag` es el «origen», p. ej. `engine/anchor`.
pub fn engine(tag: &str, msg: impl Into<String>) {
    with(|b| b.push(LogKind::Engine, tag.to_owned(), msg.into()));
}

pub fn error(tag: &str, msg: impl Into<String>) {
    with(|b| b.push(LogKind::Error, tag.to_owned(), msg.into()));
}

/// Registra un evento de motor solo cuando `value` cambia respecto a la última llamada con la misma `key`
/// (la primera llamada solo fija el valor inicial si es `false`; así «desconectado» al arrancar no es ruido).
pub fn edge(key: &'static str, value: bool, tag: &str, on: &str, off: &str) {
    with(|b| b.edge(key, value, tag, on, off));
}

pub fn since(cursor: u64) -> LogBatch {
    let mut g = bus().lock().unwrap_or_else(|p| p.into_inner());
    g.since(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(addr: &str, v: f32) -> OscMessage {
        OscMessage { addr: addr.into(), args: vec![OscType::Float(v)] }
    }

    #[test]
    fn cursor_returns_only_new_entries() {
        let mut b = LogBus::new();
        b.push(LogKind::Engine, "a".into(), "uno".into());
        b.push(LogKind::Engine, "a".into(), "dos".into());
        let first = b.since(0);
        assert_eq!(first.entries.len(), 2);
        assert_eq!(first.dropped, 0);
        b.push(LogKind::Error, "a".into(), "tres".into());
        let next = b.since(first.next_cursor);
        assert_eq!(next.entries.len(), 1);
        assert_eq!(next.entries[0].payload, "tres");
        assert!(b.since(next.next_cursor).entries.is_empty());
    }

    #[test]
    fn ring_drops_oldest_and_reports_gap() {
        let mut b = LogBus::new();
        for i in 0..(RING_CAP + 10) {
            b.push(LogKind::OscIn, "/x".into(), i.to_string());
        }
        assert_eq!(b.ring.len(), RING_CAP);
        // Un lector atrasado en el seq 5 pierde las 10 entradas más viejas del anillo
        let batch = b.since(5);
        assert_eq!(batch.entries.len(), MAX_BATCH);
        assert_eq!(batch.next_cursor, (RING_CAP + 10) as u64);
        let first = batch.entries.first().unwrap().seq;
        assert_eq!(batch.dropped, first - 6);
        assert!(batch.dropped > 0);
    }

    #[test]
    fn payload_is_formatted_and_truncated() {
        let mut b = LogBus::new();
        b.push(LogKind::OscOut, "/p".into(), fmt_args(&msg("/p", 1.23456).args));
        assert_eq!(b.ring[0].payload, "1.235");
        let long = "é".repeat(300);
        b.push(LogKind::Engine, "e".into(), long);
        assert!(b.ring[1].payload.ends_with('…'));
        assert!(b.ring[1].payload.len() <= MAX_PAYLOAD + '…'.len_utf8());
        let multi = fmt_args(&[OscType::Float(1.0), OscType::Int(2), OscType::Bool(true)]);
        assert_eq!(multi, "[1.000, 2, true]");
    }

    #[test]
    fn rate_counts_per_second_window() {
        let mut r = Rate::new();
        let t0 = r.window_start;
        for _ in 0..60 {
            r.hit(t0);
        }
        assert_eq!(r.per_sec(t0), 0, "la ventana en curso aún no cuenta");
        let t1 = t0 + std::time::Duration::from_millis(1100);
        assert_eq!(r.per_sec(t1), 60);
        let t2 = t1 + std::time::Duration::from_millis(3000);
        assert_eq!(r.per_sec(t2), 0, "sin tráfico la tasa vuelve a 0");
    }

    #[test]
    fn edge_logs_only_on_change() {
        let mut b = LogBus::new();
        for v in [false, false, true, true, false] {
            b.edge("k", v, "t", "on", "off");
        }
        let got: Vec<_> = b.ring.iter().map(|e| e.payload.as_str()).collect();
        assert_eq!(got, ["on", "off"]);
    }
}
