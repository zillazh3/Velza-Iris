//! Sonda OSC (M0): descubre VRChat por OSCQuery, registra /usercamera/* y envía mensajes de prueba.
//! Uso: probe tree [host:port] | listen [port] | send <addr> [args…] [--to host:port] | rate <hz> <secs> [radius]
use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};
use std::io::{Read, Write};
use std::net::{TcpStream, UdpSocket};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const VRC_SEND: &str = "127.0.0.1:9000";

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let r = match a.first().map(String::as_str) {
        Some("tree") => tree(a.get(1).map(String::as_str)),
        Some("listen") => listen(a.get(1).map_or("9001", String::as_str)),
        Some("send") => send(&a[1..]),
        Some("rate") => rate(&a[1..]),
        Some("latency") => latency(&a[1..]),
        _ => Err("uso: probe tree [host:port] | listen [port] | send <addr> [args…] [--to host:port] | rate <hz> <secs> [radius] | latency [step_m]".into()),
    };
    if let Err(e) = r {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

// ---------- OSCQuery ----------

fn http_get(addr: &str, path: &str) -> Res<String> {
    let mut s = TcpStream::connect(addr)?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    // HTTP/1.0 evita chunked encoding.
    write!(s, "GET {path} HTTP/1.0\r\nHost: {addr}\r\n\r\n")?;
    let mut raw = String::new();
    s.read_to_string(&mut raw)?;
    let (head, body) = raw.split_once("\r\n\r\n").ok_or("respuesta HTTP inválida")?;
    if !head.starts_with("HTTP/1.0 200") && !head.starts_with("HTTP/1.1 200") {
        return Err(format!("{path}: {}", head.lines().next().unwrap_or("")).into());
    }
    Ok(body.to_string())
}

fn discover() -> Res<String> {
    let mdns = mdns_sd::ServiceDaemon::new()?;
    let rx = mdns.browse("_oscjson._tcp.local.")?;
    println!("buscando VRChat por mDNS (10 s)…");
    let end = Instant::now() + Duration::from_secs(10);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        if let Ok(mdns_sd::ServiceEvent::ServiceResolved(r)) = rx.recv_timeout(left) {
            println!("  servicio: {} puerto {}", r.get_fullname(), r.get_port());
            if r.get_fullname().contains("VRChat-Client") {
                return Ok(format!("127.0.0.1:{}", r.get_port()));
            }
        }
    }
    Err("VRChat no apareció; ¿OSC habilitado? Pasa host:port manualmente: probe tree 127.0.0.1:<puerto>".into())
}

fn tree(addr: Option<&str>) -> Res<()> {
    let addr = match addr {
        Some(a) => a.to_string(),
        None => discover()?,
    };
    println!("HOST_INFO: {}", http_get(&addr, "/?HOST_INFO")?.trim());
    let body = http_get(&addr, "/usercamera")?;
    let json: serde_json::Value = serde_json::from_str(&body)?;
    let out = format!("{ROOT}/../../docs/oscquery-usercamera.json");
    std::fs::write(&out, serde_json::to_string_pretty(&json)?)?;
    println!("árbol guardado en {out}\n");
    summarize(&json);
    Ok(())
}

fn summarize(n: &serde_json::Value) {
    if let (Some(p), Some(t)) = (n["FULL_PATH"].as_str(), n["TYPE"].as_str()) {
        println!("{p:<40} {t:<6} acceso={} rango={} valor={}", n["ACCESS"], n["RANGE"], n["VALUE"]);
    }
    if let Some(c) = n["CONTENTS"].as_object() {
        c.values().for_each(summarize);
    }
}

// ---------- OSC ----------

fn fmt_arg(a: &OscType) -> String {
    match a {
        OscType::Float(f) => format!("f:{f}"),
        OscType::Int(i) => format!("i:{i}"),
        OscType::Bool(b) => format!("b:{b}"),
        OscType::String(s) => format!("s:{s:?}"),
        o => format!("{o:?}"),
    }
}

fn fmt_msg(m: &OscMessage) -> String {
    let args: Vec<_> = m.args.iter().map(fmt_arg).collect();
    format!("{} [{}]", m.addr, args.join(", "))
}

fn stamp() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let s = d.as_secs() % 86400;
    format!("{:02}:{:02}:{:02}.{:03}", s / 3600, s / 60 % 60, s % 60, d.subsec_millis())
}

fn flatten(p: OscPacket, out: &mut Vec<OscMessage>) {
    match p {
        OscPacket::Message(m) => out.push(m),
        OscPacket::Bundle(b) => b.content.into_iter().for_each(|c| flatten(c, out)),
    }
}

/// Bloquea leyendo UDP y llama `f` por cada mensaje OSC.
fn recv_loop(sock: &UdpSocket, mut f: impl FnMut(OscMessage)) {
    let mut buf = [0u8; 65536];
    loop {
        let Ok(n) = sock.recv(&mut buf) else { continue };
        let Ok((_, pkt)) = decoder::decode_udp(&buf[..n]) else { continue };
        let mut msgs = Vec::new();
        flatten(pkt, &mut msgs);
        msgs.into_iter().for_each(&mut f);
    }
}

fn bind(port: &str) -> Res<UdpSocket> {
    UdpSocket::bind(format!("127.0.0.1:{port}"))
        .map_err(|e| format!("no pude abrir UDP {port} ({e}); ¿otra app OSC lo usa?").into())
}

fn listen(port: &str) -> Res<()> {
    let sock = bind(port)?;
    let dir = format!("{ROOT}/logs");
    std::fs::create_dir_all(&dir)?;
    let path = format!("{dir}/{}.log", SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs());
    let mut log = std::fs::File::create(&path)?;
    println!("escuchando UDP {port}, log en {path} (Ctrl+C para salir)");
    recv_loop(&sock, |m| {
        if m.addr.starts_with("/usercamera/") {
            let line = format!("{} {}", stamp(), fmt_msg(&m));
            println!("{line}");
            let _ = writeln!(log, "{line}");
        }
    });
    Ok(())
}

/// `true/false` → bool, `i:N` → int, número → float, otro → string.
fn parse_args(args: &[String]) -> Vec<OscType> {
    args.iter()
        .map(|a| match a.as_str() {
            "true" => OscType::Bool(true),
            "false" => OscType::Bool(false),
            _ => match a.strip_prefix("i:").map(str::parse) {
                Some(Ok(i)) => OscType::Int(i),
                _ => a.parse().map_or_else(|_| OscType::String(a.clone()), OscType::Float),
            },
        })
        .collect()
}

fn send_to(sock: &UdpSocket, to: &str, addr: &str, args: Vec<OscType>) -> Res<()> {
    let pkt = OscPacket::Message(OscMessage { addr: addr.into(), args });
    sock.send_to(&encoder::encode(&pkt)?, to)?;
    Ok(())
}

fn send(a: &[String]) -> Res<()> {
    let (a, to) = match a.iter().position(|x| x == "--to") {
        Some(i) => (&a[..i], a.get(i + 1).map_or(VRC_SEND, String::as_str)),
        None => (a, VRC_SEND),
    };
    let addr = a.first().ok_or("falta <addr>")?;
    let args = parse_args(&a[1..]);
    println!("→ {to} {addr} {:?}", args);
    send_to(&UdpSocket::bind("127.0.0.1:0")?, to, addr, args)
}

// ---------- rate ----------

/// Orbita el origen de la última Pose recibida a `hz` durante `secs` y mide jitter de envío y ecos.
fn rate(a: &[String]) -> Res<()> {
    let hz: f64 = a.first().ok_or("falta <hz>")?.parse()?;
    let secs: f64 = a.get(1).ok_or("falta <secs>")?.parse()?;
    let radius: f32 = a.get(2).map_or(Ok(0.1), |r| r.parse())?;

    let rx = bind("9001")?;
    let pose: Arc<Mutex<Option<Vec<f32>>>> = Arc::default();
    let echoes = Arc::new(AtomicUsize::new(0));
    {
        let (pose, echoes) = (pose.clone(), echoes.clone());
        std::thread::spawn(move || {
            recv_loop(&rx, |m| {
                if m.addr == "/usercamera/Pose" {
                    echoes.fetch_add(1, Ordering::Relaxed);
                    let v: Vec<f32> = m.args.iter().filter_map(|x| x.clone().float()).collect();
                    *pose.lock().unwrap() = Some(v);
                }
            })
        });
    }

    println!("esperando una Pose de VRChat (mueve la cámara un poco, 15 s)…");
    let t0 = Instant::now();
    let base = loop {
        if let Some(p) = pose.lock().unwrap().clone() {
            break p;
        }
        if t0.elapsed() > Duration::from_secs(15) {
            return Err("no llegó /usercamera/Pose; ¿cámara abierta?".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    println!("pose base ({} floats): {base:?}", base.len());
    if base.len() < 6 {
        return Err("Pose con menos de 6 floats; revisa el formato con `listen` antes de usar `rate`".into());
    }

    let tx = UdpSocket::bind("127.0.0.1:0")?;
    let period = Duration::from_secs_f64(1.0 / hz);
    let n = (hz * secs) as u32;
    let echoes_before = echoes.load(Ordering::Relaxed);
    let start = Instant::now();
    let (mut sum, mut max) = (0f64, 0f64);
    for i in 0..n {
        let deadline = start + period * i;
        // sleep grueso + spin fino: el timer de Windows tiene ~15 ms de resolución.
        if let Some(d) = deadline.checked_duration_since(Instant::now()) {
            if d > Duration::from_millis(2) {
                std::thread::sleep(d - Duration::from_millis(2));
            }
        }
        while Instant::now() < deadline {
            std::hint::spin_loop();
        }
        let late = Instant::now().duration_since(deadline).as_secs_f64() * 1e3;
        sum += late;
        max = max.max(late);
        let ang = i as f32 / hz as f32 * std::f32::consts::TAU / 4.0; // vuelta cada 4 s
        let mut p = base.clone();
        p[0] += radius * ang.sin();
        p[2] += radius * (ang.cos() - 1.0);
        send_to(&tx, VRC_SEND, "/usercamera/Pose", p.into_iter().map(OscType::Float).collect())?;
    }
    std::thread::sleep(Duration::from_millis(500));
    let got = echoes.load(Ordering::Relaxed) - echoes_before;
    println!(
        "{hz} Hz x {secs} s: enviados={n} retraso medio={:.2} ms máx={max:.2} ms ecos={got}",
        sum / n as f64
    );
    // Devuelve la cámara a donde estaba.
    send_to(&tx, VRC_SEND, "/usercamera/Pose", base.into_iter().map(OscType::Float).collect())
}

/// Mide la latencia de transporte (τ_d: 10% del escalón) y el tiempo de establecimiento (τ_s: 90% del escalón)
/// enviando un desplazamiento controlado sobre `/usercamera/Pose`. No requiere VRC+.
fn latency(a: &[String]) -> Res<()> {
    let step_m: f32 = a.first().map_or(Ok(0.20), |s| s.parse())?;
    let rx = bind("9001")?;
    let latest: Arc<Mutex<Option<(Instant, Vec<f32>)>>> = Arc::default();
    {
        let latest = latest.clone();
        std::thread::spawn(move || {
            recv_loop(&rx, |m| {
                if m.addr == "/usercamera/Pose" {
                    let v: Vec<f32> = m.args.iter().filter_map(|x| x.clone().float()).collect();
                    if v.len() >= 6 {
                        *latest.lock().unwrap() = Some((Instant::now(), v));
                    }
                }
            })
        });
    }

    println!("esperando Pose base de VRChat (mueve ligeramente la cámara, 15 s)…");
    let t0 = Instant::now();
    let base = loop {
        if let Some((_, p)) = latest.lock().unwrap().clone() {
            break p;
        }
        if t0.elapsed() > Duration::from_secs(15) {
            return Err("no llegó /usercamera/Pose; ¿cámara abierta?".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    let tx = UdpSocket::bind("127.0.0.1:0")?;
    let mut target = base.clone();
    target[0] += step_m;

    let sent_at = Instant::now();
    send_to(&tx, VRC_SEND, "/usercamera/Pose", target.into_iter().map(OscType::Float).collect())?;

    let (mut t10_ms, mut t90_ms) = (None, None);
    while sent_at.elapsed() < Duration::from_secs(2) {
        if let Some((stamp, p)) = latest.lock().unwrap().clone() {
            if stamp >= sent_at {
                let dx = (p[0] - base[0]).abs();
                if t10_ms.is_none() && dx >= step_m.abs() * 0.10 {
                    t10_ms = Some(stamp.duration_since(sent_at).as_secs_f64() * 1e3);
                }
                if t90_ms.is_none() && dx >= step_m.abs() * 0.90 {
                    t90_ms = Some(stamp.duration_since(sent_at).as_secs_f64() * 1e3);
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }

    println!(
        "escalón {:.2} m -> τ_d (10%) = {:?} ms, τ_s (90%) = {:?} ms",
        step_m, t10_ms, t90_ms
    );
    send_to(&tx, VRC_SEND, "/usercamera/Pose", base.into_iter().map(OscType::Float).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_infers_types() {
        let a: Vec<String> = ["true", "i:3", "1.5", "hola"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            parse_args(&a),
            vec![OscType::Bool(true), OscType::Int(3), OscType::Float(1.5), OscType::String("hola".into())]
        );
    }
}
