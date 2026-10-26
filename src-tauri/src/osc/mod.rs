//! Cliente OSC hacia VRChat: envío con clamps y recepción de ecos en un hilo.
//! Soporta degradación limpia a modo de Control Abierto (`rx_bound = false`) si el puerto UDP 9001 está ocupado.
pub mod protocol;
pub mod query;

use protocol::{encode, Pose, Value};
use rosc::{decoder, encoder, OscMessage, OscPacket};
use std::net::{SocketAddr, UdpSocket};

/// Puerto en el que VRChat envía por defecto.
pub const DEFAULT_LISTEN_PORT: u16 = 9001;

pub struct Osc {
    tx: UdpSocket,
    target: SocketAddr,
    /// `true` si pudimos abrir el socket UDP de recepción de ecos; `false` si operamos en lazo abierto.
    pub rx_bound: bool,
    pub listen_port: u16,
}

fn flatten(p: OscPacket, out: &mut Vec<OscMessage>) {
    match p {
        OscPacket::Message(m) => out.push(m),
        OscPacket::Bundle(b) => b.content.into_iter().for_each(|c| flatten(c, out)),
    }
}

impl Osc {
    /// Abre el puerto UDP por defecto (9001) y llama `on_msg` por cada mensaje recibido.
    /// Si otra aplicación OSC ya ocupa el puerto 9001, continúa en modo Control Abierto (`rx_bound = false`).
    pub fn start(target: SocketAddr, on_msg: impl FnMut(OscMessage) + Send + 'static) -> Result<Osc, String> {
        Self::start_with_port(target, DEFAULT_LISTEN_PORT, on_msg)
    }

    pub fn start_with_port(
        target: SocketAddr,
        listen_port: u16,
        mut on_msg: impl FnMut(OscMessage) + Send + 'static,
    ) -> Result<Osc, String> {
        let tx = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let rx_bound = match UdpSocket::bind(("127.0.0.1", listen_port)) {
            Ok(rx) => {
                std::thread::spawn(move || {
                    let mut buf = [0u8; 65536];
                    loop {
                        let n = match rx.recv(&mut buf) {
                            Ok(n) => n,
                            Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                                // En Windows (Winsock), 10054 (WSAECONNRESET) ocurre en UDP tras un ICMP; ignorar y continuar escuchando
                                continue;
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                                continue;
                            }
                            Err(e) => {
                                eprintln!("[OSC] Error en recepción UDP puerto {}: {}", listen_port, e);
                                crate::logbus::error("osc/rx", format!("UDP {listen_port}: {e}"));
                                std::thread::sleep(std::time::Duration::from_millis(10));
                                continue;
                            }
                        };
                        let Ok((_, pkt)) = decoder::decode_udp(&buf[..n]) else { continue };
                        let mut msgs = Vec::new();
                        flatten(pkt, &mut msgs);
                        for m in msgs {
                            crate::logbus::osc_in(&m);
                            on_msg(m);
                        }
                    }
                });
                true
            }
            Err(e) => {
                eprintln!("[OSC] No se pudo enlazar socket de recepción en 127.0.0.1:{} ({}); operando en lazo abierto.", listen_port, e);
                crate::logbus::error("osc/rx", format!("no se pudo enlazar 127.0.0.1:{listen_port} ({e}); lazo abierto"));
                false
            }
        };
        Ok(Osc {
            tx,
            target,
            rx_bound,
            listen_port,
        })
    }

    fn send(&self, addr: String, args: Vec<rosc::OscType>) -> Result<(), String> {
        self.send_packet(&OscPacket::Message(OscMessage { addr, args }))
    }

    /// Envío directo de un paquete OSC sin conversión de protocolo de cámara.
    pub fn send_raw(&self, addr: String, args: Vec<rosc::OscType>) -> Result<(), String> {
        self.send(addr, args)
    }

    /// Envío directo de un paquete OSC (Message o Bundle) codificado.
    pub fn send_packet(&self, packet: &OscPacket) -> Result<(), String> {
        let bytes = encoder::encode(packet).map_err(|e| e.to_string())?;
        let sent = self.tx.send_to(&bytes, self.target).map(drop).map_err(|e| e.to_string());
        match &sent {
            Ok(()) => crate::logbus::osc_out_packet(packet),
            Err(e) => crate::logbus::error("osc/tx", e.clone()),
        }
        sent
    }

    /// Envío atómico de múltiples mensajes OSC dentro de un mismo OSC Bundle.
    pub fn send_bundle(&self, messages: Vec<OscMessage>) -> Result<(), String> {
        let content = messages.into_iter().map(OscPacket::Message).collect();
        let bundle = rosc::OscBundle {
            timetag: rosc::OscTime { seconds: 0, fractional: 0 },
            content,
        };
        self.send_packet(&OscPacket::Bundle(bundle))
    }

    /// Slider, toggle, modo o acción; el valor se valida y recorta en `protocol::encode`.
    pub fn set(&self, name: &str, value: Value) -> Result<(), String> {
        let (addr, args) = encode(name, value)?;
        self.send(addr, args)
    }

    pub fn pose(&self, pose: &Pose) -> Result<(), String> {
        self.send(Pose::ADDR.into(), pose.to_args()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_open_loop_when_listen_port_is_busy() {
        let blocker = UdpSocket::bind("127.0.0.1:0").unwrap();
        let busy_port = blocker.local_addr().unwrap().port();
        let target: SocketAddr = "127.0.0.1:9000".parse().unwrap();

        let osc = Osc::start_with_port(target, busy_port, |_| {}).unwrap();
        assert!(!osc.rx_bound, "debe degradar a modo abierto si el puerto está ocupado");
        assert_eq!(osc.listen_port, busy_port);
    }
}
