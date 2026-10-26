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
        Some("combo") => combo(&a[1..]),
        Some("lag") => lag(&a[1..]),
        _ => Err("uso: probe tree [host:port] | listen [port] | send <addr> [args…] [--to host:port] | rate <hz> <secs> [radius] | latency [step_m] | combo <secs> [avatar]".into()),
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

// ---------- combo ----------

/// Canales ópticos barridos junto con la Pose: (endpoint, mínimo, máximo, periodo en s).
const SWEEPS: &[(&str, f32, f32, f32)] = &[
    ("Zoom", 25.0, 120.0, 3.0),
    ("Exposure", -2.0, 2.0, 5.0),
    ("FocalDistance", 0.5, 4.0, 4.0),
    ("Aperture", 2.0, 16.0, 6.0),
];

/// Pose orbitando + barrido simultáneo de Zoom/Exposure/Focal/Aperture a 60 Hz (como la reproducción en vivo).
/// Con `avatar` añade en paralelo `/avatar/parameters/iris_*` (cámara a 60 Hz + multiplexado de 5 huesos).
/// Reporta por canal: enviados, ecos, último valor enviado vs. último eco y mayor desfase observado.
fn combo(a: &[String]) -> Res<()> {
    let secs: f64 = a.first().map_or(Ok(8.0), |s| s.parse())?;
    let with_avatar = a.iter().any(|x| x == "avatar");
    let rx = bind("9001")?;
    type Stats = std::collections::HashMap<String, (usize, Option<f32>)>;
    let pose: Arc<Mutex<Option<Vec<f32>>>> = Arc::default();
    let echo: Arc<Mutex<Stats>> = Arc::default();
    {
        let (pose, echo) = (pose.clone(), echo.clone());
        std::thread::spawn(move || {
            recv_loop(&rx, |m| {
                let Some(name) = m.addr.strip_prefix("/usercamera/") else { return };
                if name == "Pose" {
                    let v: Vec<f32> = m.args.iter().filter_map(|x| x.clone().float()).collect();
                    *pose.lock().unwrap() = Some(v);
                }
                let val = m.args.first().and_then(|x| x.clone().float());
                let mut e = echo.lock().unwrap();
                let s = e.entry(name.to_string()).or_insert((0, None));
                s.0 += 1;
                s.1 = val.or(s.1);
            })
        });
    }
    println!("esperando una Pose de VRChat (mueve la cámara un poco, 15 s)…");
    let t0 = Instant::now();
    let base = loop {
        if let Some(p) = pose.lock().unwrap().clone().filter(|p| p.len() >= 6) {
            break p;
        }
        if t0.elapsed() > Duration::from_secs(15) {
            return Err("no llegó /usercamera/Pose; ¿cámara abierta?".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    println!("Pose recibida: SUELTA la cámara y no toques nada (inicia en 3 s)…");
    std::thread::sleep(Duration::from_secs(3));
    echo.lock().unwrap().clear();
    let tx = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mux_cycles = Arc::new(AtomicUsize::new(0));
    let mut handle = None;
    if with_avatar {
        send_to(&tx, VRC_SEND, "/avatar/parameters/iris_enable", vec![OscType::Bool(true)])?;
        // Multiplexado del Path (mismos tiempos que avatar/service.rs) en un hilo aparte.
        let (tx, stop, cyc) = (tx.clone(), stop.clone(), mux_cycles.clone());
        handle = Some(std::thread::spawn(move || {
            let int = |a: &str, v: i32| send_to(&tx, VRC_SEND, a, vec![OscType::Int(v)]);
            let fl = |a: &str, v: f32| send_to(&tx, VRC_SEND, a, vec![OscType::Float(v)]);
            while !stop.load(Ordering::Relaxed) {
                let _ = int("/avatar/parameters/iris_wp_index", 0);
                std::thread::sleep(Duration::from_millis(25));
                for i in 1..=5 {
                    for ax in ["x", "y", "z"] {
                        let _ = fl(&format!("/avatar/parameters/iris_wp_{ax}"), i as f32 * 0.05);
                    }
                    std::thread::sleep(Duration::from_millis(30));
                    let _ = int("/avatar/parameters/iris_wp_index", i);
                    std::thread::sleep(Duration::from_millis(30));
                    let _ = int("/avatar/parameters/iris_wp_index", 0);
                    std::thread::sleep(Duration::from_millis(20));
                }
                cyc.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    let period = Duration::from_secs_f64(1.0 / 60.0);
    let n = (60.0 * secs) as u32;
    let start = Instant::now();
    let mut last_sent: std::collections::HashMap<&str, f32> = Default::default();
    // Desfase máximo |eco - enviado| por canal (ignorando latencia: se compara con el valor enviado 3 frames antes).
    let mut history: std::collections::HashMap<&str, Vec<f32>> = Default::default();
    let mut max_lag: std::collections::HashMap<&str, f32> = Default::default();
    for i in 0..n {
        let deadline = start + period * i;
        if let Some(d) = deadline.checked_duration_since(Instant::now()) {
            if d > Duration::from_millis(2) {
                std::thread::sleep(d - Duration::from_millis(2));
            }
        }
        while Instant::now() < deadline {
            std::hint::spin_loop();
        }
        let t = i as f32 / 60.0;
        let ang = t * std::f32::consts::TAU / 4.0;
        let mut p = base.clone();
        p[0] += 0.3 * ang.sin();
        p[2] += 0.3 * (ang.cos() - 1.0);
        p[4] = (base[4] + t * 20.0) % 360.0;
        send_to(&tx, VRC_SEND, "/usercamera/Pose", p.iter().copied().map(OscType::Float).collect())?;
        for (name, lo, hi, per) in SWEEPS {
            let v = lo + (hi - lo) * 0.5 * (1.0 + (t / per * std::f32::consts::TAU).sin());
            send_to(&tx, VRC_SEND, &format!("/usercamera/{name}"), vec![OscType::Float(v)])?;
            last_sent.insert(name, v);
            let h = history.entry(name).or_default();
            h.push(v);
            if let Some((_, Some(e))) = echo.lock().unwrap().get(*name) {
                // El eco debe coincidir con alguno de los últimos 12 valores enviados (≈200 ms de latencia).
                let best = h.iter().rev().take(12).map(|s| (s - e).abs()).fold(f32::MAX, f32::min);
                let m = max_lag.entry(name).or_insert(0.0);
                *m = m.max(best);
            }
        }
        if with_avatar {
            for (k, v) in ["x", "y", "z"].iter().zip([0.1, 0.2, 0.3]) {
                send_to(&tx, VRC_SEND, &format!("/avatar/parameters/iris_cam_{k}"), vec![OscType::Float(v * ang.sin())])?;
            }
            for (k, v) in ["rx", "ry", "rz"].iter().zip([0.0, ang.sin(), 0.0]) {
                send_to(&tx, VRC_SEND, &format!("/avatar/parameters/iris_cam_{k}"), vec![OscType::Float(v)])?;
            }
        }
    }
    stop.store(true, Ordering::Relaxed);
    if let Some(h) = handle {
        let _ = h.join();
    }
    std::thread::sleep(Duration::from_millis(600));
    println!("{n} frames a 60 Hz en {secs} s (avatar={with_avatar}, ciclos de multiplexado={})", mux_cycles.load(Ordering::Relaxed));
    let e = echo.lock().unwrap();
    for (name, ..) in SWEEPS {
        let (cnt, val) = e.get(*name).copied().unwrap_or((0, None));
        println!(
            "  {name:<14} enviado_final={:>8.3} eco_final={:>8?} ecos={cnt:<5} desfase_max_vs_historial={:.3}",
            last_sent[name],
            val,
            max_lag.get(name).copied().unwrap_or(f32::NAN)
        );
    }
    println!("  Pose ecos={}", e.get("Pose").map_or(0, |s| s.0));
    // Restaura valores neutros.
    for (name, lo, hi, _) in SWEEPS {
        send_to(&tx, VRC_SEND, &format!("/usercamera/{name}"), vec![OscType::Float((lo + hi) / 2.0)])?;
    }
    if with_avatar {
        send_to(&tx, VRC_SEND, "/avatar/parameters/iris_enable", vec![OscType::Bool(false)])?;
    }
    send_to(&tx, VRC_SEND, "/usercamera/Pose", base.into_iter().map(OscType::Float).collect())
}

// ---------- lag ----------

/// `lag <ep1,ep2,..> <hz> <secs>`: cada tick envía un valor único creciente a TODOS los endpoints (Pose = `pose`) y mide
/// por endpoint cuánto tarda su eco. Revela colas internas de VRChat (retraso creciente = envío > capacidad de proceso).
fn lag(a: &[String]) -> Res<()> {
    let names: Vec<String> = a.first().ok_or("falta <ep1,ep2,…> (p.ej. Zoom,Exposure)")?.split(',').map(String::from).collect();
    let hz: f64 = a.get(1).ok_or("falta <hz>")?.parse()?;
    let secs: f64 = a.get(2).ok_or("falta <secs>")?.parse()?;
    let div: usize = a.get(3).map_or(Ok(1), |s| s.parse())?; // sliders solo cada `div` ticks (Pose siempre)
    let params = |n: &str| match n {
        "Zoom" => (30.0f32, 0.01f32),
        "Exposure" => (-4.0, 0.001),
        "Aperture" => (2.0, 0.01),
        _ => (0.5, 0.005),
    };
    let rx = bind("9001")?;
    type Log = std::collections::HashMap<String, Vec<(Instant, f32)>>;
    let echoes: Arc<Mutex<Log>> = Arc::default();
    {
        let echoes = echoes.clone();
        std::thread::spawn(move || {
            recv_loop(&rx, |m| {
                let Some(n) = m.addr.strip_prefix("/usercamera/") else { return };
                // Pose: se identifica por el campo X (índice 0); el resto por su único float.
                if let Some(v) = m.args.first().and_then(|x| x.clone().float()) {
                    echoes.lock().unwrap().entry(n.to_string()).or_default().push((Instant::now(), v));
                }
            })
        });
    }
    let base_pose = if false {
        println!("mueve la cámara para obtener Pose base…");
        let t0 = Instant::now();
        loop {
            if let Some(v) = echoes.lock().unwrap().get("Pose").and_then(|v| v.last()).map(|x| x.1) {
                let _ = v;
                break;
            }
            if t0.elapsed() > Duration::from_secs(15) {
                return Err("sin Pose".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        echoes.lock().unwrap().clear();
        Some(())
    } else {
        None
    };
    let _ = base_pose;
    let tx = UdpSocket::bind("127.0.0.1:0")?;
    let n = (hz * secs) as usize;
    let period = Duration::from_secs_f64(1.0 / hz);
    let start = Instant::now();
    let mut sent_at: Vec<Instant> = Vec::with_capacity(n);
    for i in 0..n {
        let d = start + period * i as u32;
        if let Some(w) = d.checked_duration_since(Instant::now()) {
            if w > Duration::from_millis(2) {
                std::thread::sleep(w - Duration::from_millis(2));
            }
        }
        while Instant::now() < d {
            std::hint::spin_loop();
        }
        sent_at.push(Instant::now());
        for name in &names {
            if name == "Pose" {
                // X crece 0.001 m por tick: identificable; el resto fijo.
                let args = [1.0 + 0.001 * i as f32, 1.6, 2.0 + 0.3 * (i as f32 / 60.0 * 1.5).sin(), 10.0 * (i as f32 / 30.0).sin(), (90.0 + i as f32 * 0.33) % 360.0, 0.0].map(OscType::Float).to_vec();
                send_to(&tx, VRC_SEND, "/usercamera/Pose", args)?;
            } else if i % div == 0 {
                let (b, s) = params(name);
                send_to(&tx, VRC_SEND, &format!("/usercamera/{name}"), vec![OscType::Float(b + s * i as f32)])?;
            }
        }
    }
    println!("{names:?}: {n} ticks a {hz} Hz; drenando ecos (hasta 30 s)…");
    let (mut last, mut idle) = (0, Instant::now());
    while idle.elapsed() < Duration::from_secs(4) && start.elapsed() < Duration::from_secs(30 + secs as u64) {
        std::thread::sleep(Duration::from_millis(100));
        let c: usize = echoes.lock().unwrap().values().map(Vec::len).sum();
        if c != last {
            last = c;
            idle = Instant::now();
        }
    }
    let e = echoes.lock().unwrap();
    for name in &names {
        let (b, s) = if name == "Pose" { (1.0, 0.001) } else { params(name) };
        let mut seen = std::collections::HashMap::new();
        for (t, v) in e.get(name.as_str()).map(Vec::as_slice).unwrap_or(&[]) {
            seen.entry(((v - b) / s).round() as i64).or_insert(*t);
        }
        let mut lags: Vec<(usize, f64)> = (0..n)
            .filter_map(|i| seen.get(&(i as i64)).map(|t| (i, t.duration_since(sent_at[i]).as_secs_f64() * 1e3)))
            .collect();
        let total = e.get(name.as_str()).map_or(0, Vec::len);
        print!("  {name:<14} ecos={total:<5} únicos={:<4} perdidos={:<4}", lags.len(), n - lags.len());
        if let (Some(&f), Some(&l)) = (lags.first(), lags.last()) {
            let max = lags.iter().map(|x| x.1).fold(0.0, f64::max);
            lags.sort_by(|x, y| x.1.total_cmp(&y.1));
            print!(" retraso ms: primero={:.0} último={:.0} mediana={:.0} máx={max:.0}", f.1, l.1, lags[lags.len() / 2].1);
        }
        println!();
    }
    drop(e);
    for (name, v) in [("Zoom", 45.0), ("Exposure", 0.0), ("FocalDistance", 1.5), ("Aperture", 15.0)] {
        if names.iter().any(|x| x == name) {
            send_to(&tx, VRC_SEND, &format!("/usercamera/{name}"), vec![OscType::Float(v)])?;
        }
    }
    Ok(())
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
