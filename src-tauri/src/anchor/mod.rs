//! Modo B: ancla calibrada por el usuario (ojos + yaw) y encuadres relativos a ella.
//! Ejes de Unity/VRChat (medidos en M0): y arriba; adelante = (sin yaw, cos yaw) en (x, z); pitch + = mirar abajo.
use crate::osc::protocol::Pose;

/// Origen de los presets: posición de los ojos del jugador y hacia dónde mira (yaw en grados).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub eye: [f32; 3],
    pub yaw: f32,
}

/// Encuadre relativo al ancla. La cámara siempre mira a los ojos del jugador; `pitch`/`roll` se suman.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    /// Distancia horizontal a los ojos (m).
    pub distance: f32,
    /// Altura respecto a los ojos (m).
    pub height: f32,
    /// Posición alrededor del jugador (°): 0 = frente a él, + = hacia su derecha.
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
}

impl Anchor {
    /// "Calibrar": el usuario acerca la cámara a su cara mirando al frente; esa pose es el origen.
    pub fn from_pose(p: &Pose) -> Anchor {
        Anchor { eye: p.pos, yaw: p.rot[1] }
    }

    /// Pose de mundo de la cámara para `f`.
    pub fn pose(&self, f: &Frame) -> Pose {
        let a = (self.yaw + f.yaw).to_radians();
        let [x, y, z] = self.eye;
        Pose {
            pos: [x + f.distance * a.sin(), y + f.height, z + f.distance * a.cos()],
            rot: [
                f.height.atan2(f.distance).to_degrees() + f.pitch,
                (self.yaw + f.yaw + 180.0).rem_euclid(360.0),
                f.roll,
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EYE: [f32; 3] = [0.0, 1.6, 0.0];

    fn frame(distance: f32, height: f32, yaw: f32) -> Frame {
        Frame { distance, height, yaw, pitch: 0.0, roll: 0.0 }
    }

    fn assert_pose(p: Pose, pos: [f32; 3], rot: [f32; 3]) {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        assert!(p.pos.iter().zip(pos).all(|(a, b)| near(*a, b)), "pos {:?} != {pos:?}", p.pos);
        assert!(p.rot.iter().zip(rot).all(|(a, b)| near(*a, b)), "rot {:?} != {rot:?}", p.rot);
    }

    #[test]
    fn front_and_right_of_player() {
        let a = Anchor { eye: EYE, yaw: 0.0 };
        // de frente, mirando de vuelta al jugador (yaw 180)
        assert_pose(a.pose(&frame(2.0, 0.0, 0.0)), [0.0, 1.6, 2.0], [0.0, 180.0, 0.0]);
        // a su derecha (+x) mirando hacia -x
        assert_pose(a.pose(&frame(2.0, 0.0, 90.0)), [2.0, 1.6, 0.0], [0.0, 270.0, 0.0]);
    }

    #[test]
    fn follows_anchor_yaw() {
        // jugador mirando a +x: "de frente" queda en +x
        let a = Anchor { eye: EYE, yaw: 90.0 };
        assert_pose(a.pose(&frame(2.0, 0.0, 0.0)), [2.0, 1.6, 0.0], [0.0, 270.0, 0.0]);
    }

    #[test]
    fn height_tilts_and_offsets_add() {
        let a = Anchor { eye: EYE, yaw: 0.0 };
        // arriba y a 1 m: mira hacia abajo 45°
        assert_pose(a.pose(&frame(1.0, 1.0, 0.0)), [0.0, 2.6, 1.0], [45.0, 180.0, 0.0]);
        // abajo: contrapicado (pitch negativo = mirar arriba)
        assert_pose(a.pose(&frame(1.8, -0.6, 0.0)), [0.0, 1.0, 1.8], [-18.434, 180.0, 0.0]);
        let f = Frame { pitch: 5.0, roll: 12.0, ..frame(1.0, 1.0, 0.0) };
        assert_pose(a.pose(&f), [0.0, 2.6, 1.0], [50.0, 180.0, 12.0]);
    }

    #[test]
    fn calibrates_from_pose() {
        let p = Pose { pos: [3.0, 1.62, -2.0], rot: [4.0, -88.0, 0.0] };
        assert_eq!(Anchor::from_pose(&p), Anchor { eye: [3.0, 1.62, -2.0], yaw: -88.0 });
    }
}
