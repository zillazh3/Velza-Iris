//! Cliente OSC hacia VRChat: envío con clamps y recepción de ecos en un hilo.
pub mod protocol;
pub mod query;

use protocol::{encode, Pose, Value};
use rosc::{decoder, encoder, OscMessage, OscPacket};
use std::net::{SocketAddr, UdpSocket};

/// Puerto en el que VRChat envía por defecto.
const LISTEN_PORT: u16 = 9001;

pub struct Osc {
    tx: UdpSocket,
    target: SocketAddr,
}

fn flatten(p: OscPacket, out: &mut Vec<OscMessage>) {
    match p {
        OscPacket::Message(m) => out.push(m),
        OscPacket::Bundle(b) => b.content.into_iter().for_each(|c| flatten(c, out)),
    }
}

impl Osc {
    /// Abre el UDP 9001 y llama `on_msg` por cada mensaje recibido (desde otro hilo).
    pub fn start(target: SocketAddr, mut on_msg: impl FnMut(OscMessage) + Send + 'static) -> Result<Osc, String> {
        let rx = UdpSocket::bind(("127.0.0.1", LISTEN_PORT))
            .map_err(|e| format!("no pude abrir UDP {LISTEN_PORT} ({e}); ¿otra app OSC lo usa?"))?;
        let tx = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        std::thread::spawn(move || {
            let mut buf = [0u8; 65536];
            while let Ok(n) = rx.recv(&mut buf) {
                let Ok((_, pkt)) = decoder::decode_udp(&buf[..n]) else { continue };
                let mut msgs = Vec::new();
                flatten(pkt, &mut msgs);
                msgs.into_iter().for_each(&mut on_msg);
            }
        });
        Ok(Osc { tx, target })
    }

    fn send(&self, addr: String, args: Vec<rosc::OscType>) -> Result<(), String> {
        let bytes = encoder::encode(&OscPacket::Message(OscMessage { addr, args })).map_err(|e| e.to_string())?;
        self.tx.send_to(&bytes, self.target).map(drop).map_err(|e| e.to_string())
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
