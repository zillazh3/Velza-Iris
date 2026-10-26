// Prueba de la lógica de la vista Logs: node --test tools/logs.test.mjs (Node >= 22.6 con type stripping)
import test from "node:test";
import assert from "node:assert/strict";
import { LogStore, OSC_CAP, SYSTEM_CAP } from "../src/lib/logs.ts";

const osc = (seq, tMs, addr = "/usercamera/Pose", kind = "oscIn", payload = String(seq)) => ({ seq, tMs, kind, addr, payload });
const sys = (seq, tMs, kind = "engine") => ({ seq, tMs, kind, addr: "engine/x", payload: "evento" });

test("colapsa paquetes iguales dentro de la ventana de 1 s y abre otra fila después", () => {
  const s = new LogStore();
  // 60 paquetes en ~1 s a 60 Hz + uno justo después de la ventana
  s.ingest(Array.from({ length: 60 }, (_, i) => osc(i + 1, 1000 + i * 16)), true);
  s.ingest([osc(61, 2100)], true);
  const rows = s.rows("all", "");
  assert.equal(rows.length, 2);
  assert.equal(rows[0].count, 60);
  assert.equal(rows[0].payload, "60", "conserva el último payload");
  assert.equal(rows[0].id, 1, "la identidad es la de la primera entrada");
  assert.equal(rows[1].count, 1);
});

test("sin colapso cada paquete es una fila; entrada y salida no se mezclan", () => {
  const s = new LogStore();
  s.ingest([osc(1, 0), osc(2, 5), osc(3, 6, "/usercamera/Pose", "oscOut")], false);
  assert.equal(s.rows("all", "").length, 3);
  s.clear();
  s.ingest([osc(1, 0), osc(2, 5), osc(3, 6, "/usercamera/Pose", "oscOut")], true);
  assert.equal(s.rows("oscIn", "").length, 1);
  assert.equal(s.rows("oscOut", "").length, 1);
});

test("el spam de OSC no desplaza los eventos del sistema", () => {
  const s = new LogStore();
  s.ingest([sys(1, 0), sys(2, 1, "error")], true);
  s.ingest(Array.from({ length: OSC_CAP * 3 }, (_, i) => osc(i + 10, i, `/a/${i}`)), true);
  const rows = s.rows("all", "");
  assert.equal(s.rows("engine", "").length, 1);
  assert.equal(s.rows("error", "").length, 1);
  assert.equal(rows.filter((r) => r.kind === "oscIn").length, OSC_CAP);
  assert.equal(rows[0].id, 1, "«All» ordena por seq: los eventos viejos del sistema van primero");
});

test("anillo del sistema acotado, búsqueda y clear", () => {
  const s = new LogStore();
  s.ingest(Array.from({ length: SYSTEM_CAP + 20 }, (_, i) => sys(i + 1, i)), true);
  assert.equal(s.rows("engine", "").length, SYSTEM_CAP);
  s.ingest([osc(9999, 5, "/avatar/parameters/iris_cam_x", "oscOut", "0.123")], true);
  assert.equal(s.rows("all", "IRIS_CAM").length, 1, "búsqueda sin distinguir mayúsculas");
  assert.equal(s.rows("all", "0.123").length, 1, "también busca en el payload");
  s.clear();
  assert.equal(s.rows("all", "").length, 0);
});
