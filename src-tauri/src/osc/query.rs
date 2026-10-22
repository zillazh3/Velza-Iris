//! Descubre el cliente de VRChat por OSCQuery (mDNS `_oscjson._tcp` + `/?HOST_INFO`).
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

fn http_get(addr: &str, path: &str) -> Result<String, String> {
    let io = |e: std::io::Error| format!("{addr}{path}: {e}");
    let mut s = TcpStream::connect(addr).map_err(io)?;
    s.set_read_timeout(Some(Duration::from_secs(5))).map_err(io)?;
    // HTTP/1.0 evita chunked encoding.
    write!(s, "GET {path} HTTP/1.0\r\nHost: {addr}\r\n\r\n").map_err(io)?;
    let mut raw = String::new();
    s.read_to_string(&mut raw).map_err(io)?;
    let (head, body) = raw.split_once("\r\n\r\n").ok_or("respuesta HTTP inválida")?;
    if !head.contains(" 200") {
        return Err(format!("{path}: {}", head.lines().next().unwrap_or("")));
    }
    Ok(body.to_string())
}

/// Dirección UDP a la que VRChat espera nuestros mensajes, según su HOST_INFO.
fn parse_host_info(body: &str) -> Option<SocketAddr> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    format!("{}:{}", v["OSC_IP"].as_str()?, v["OSC_PORT"].as_u64()?).parse().ok()
}

/// Busca `VRChat-Client-*` por mDNS hasta `timeout` y devuelve su destino OSC.
/// ponytail: no nos anunciamos como servicio OSCQuery; la escucha va en el 9001 fijo (ver `Osc::start`).
pub fn vrchat_target(timeout: Duration) -> Result<SocketAddr, String> {
    let mdns = ServiceDaemon::new().map_err(|e| e.to_string())?;
    let rx = mdns.browse("_oscjson._tcp.local.").map_err(|e| e.to_string())?;
    let end = Instant::now() + timeout;
    let found = loop {
        let Some(left) = end.checked_duration_since(Instant::now()) else {
            break Err("VRChat no apareció por OSCQuery; ¿OSC habilitado?".to_string());
        };
        if let Ok(ServiceEvent::ServiceResolved(r)) = rx.recv_timeout(left) {
            if r.get_fullname().contains("VRChat-Client") {
                break Ok(r.get_port());
            }
        }
    };
    let _ = mdns.shutdown();
    let info = http_get(&format!("127.0.0.1:{}", found?), "/?HOST_INFO")?;
    parse_host_info(&info).ok_or_else(|| "HOST_INFO sin OSC_IP/OSC_PORT".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_host_info() {
        // Respuesta real de VRChat capturada en M0.
        let body = r#"{"NAME":"VRChat-Client-EC5D92","EXTENSIONS":{"ACCESS":true},"OSC_IP":"127.0.0.1","OSC_PORT":9000,"OSC_TRANSPORT":"UDP"}"#;
        assert_eq!(parse_host_info(body), Some("127.0.0.1:9000".parse().unwrap()));
        assert_eq!(parse_host_info("{}"), None);
    }
}
