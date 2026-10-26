// Lógica pura de la vista «Logs»: dos anillos en memoria (OSC / sistema) y colapso de paquetes OSC repetidos.
// Sin dependencias de Svelte ni de Tauri para poder probarla con `node --test` (ver tools/logs.test.mjs).

export type LogKind = "oscIn" | "oscOut" | "engine" | "error";
export type LogFilter = "all" | LogKind;

/** Entrada tal como la entrega el backend (`LogBatch.entries`). */
export type LogEntry = { seq: number; tMs: number; kind: LogKind; addr: string; payload: string };

export type LogBatch = {
  entries: LogEntry[];
  nextCursor: number;
  dropped: number;
  rateIn: number;
  rateOut: number;
};

/** Fila mostrada. Inmutable: al colapsar se reemplaza por otro objeto, así `{#each}` solo repinta lo que cambió. */
export type LogRow = LogEntry & {
  /** `seq` de la primera entrada del grupo: identidad estable y orden en «All». */
  id: number;
  /** Instante en que se abrió el grupo; el colapso agrupa por ventanas de `COLLAPSE_MS` desde aquí. */
  startMs: number;
  count: number;
};

export const OSC_CAP = 1000;
export const SYSTEM_CAP = 500;
export const COLLAPSE_MS = 1000;

const isOsc = (k: LogKind) => k === "oscIn" || k === "oscOut";

export class LogStore {
  private osc: LogRow[] = [];
  private system: LogRow[] = [];
  /** Última fila abierta por (tipo, dirección) para colapsar. */
  private open = new Map<string, LogRow>();

  ingest(entries: LogEntry[], collapse: boolean): void {
    for (const e of entries) {
      if (!isOsc(e.kind)) {
        this.system.push({ ...e, id: e.seq, startMs: e.tMs, count: 1 });
        if (this.system.length > SYSTEM_CAP) this.system.shift();
        continue;
      }
      const key = `${e.kind}|${e.addr}`;
      const prev = collapse ? this.open.get(key) : undefined;
      if (prev && e.tMs - prev.startMs < COLLAPSE_MS) {
        const next: LogRow = { ...prev, seq: e.seq, tMs: e.tMs, payload: e.payload, count: prev.count + 1 };
        this.replace(prev, next);
        this.open.set(key, next);
        continue;
      }
      const row: LogRow = { ...e, id: e.seq, startMs: e.tMs, count: 1 };
      this.osc.push(row);
      this.open.set(key, row);
      if (this.osc.length > OSC_CAP) this.osc.shift();
    }
  }

  /** Las filas colapsadas casi siempre están cerca del final: se busca desde ahí. */
  private replace(prev: LogRow, next: LogRow): void {
    for (let i = this.osc.length - 1; i >= 0; i--) {
      if (this.osc[i] === prev) {
        this.osc[i] = next;
        return;
      }
    }
  }

  clear(): void {
    this.osc = [];
    this.system = [];
    this.open.clear();
  }

  /** Filas filtradas, en orden de aparición. `search` busca en dirección y payload sin distinguir mayúsculas. */
  rows(filter: LogFilter, search: string): LogRow[] {
    const q = search.trim().toLowerCase();
    let src: LogRow[];
    if (filter === "all") src = mergeById(this.osc, this.system);
    else if (filter === "engine" || filter === "error") src = this.system.filter((r) => r.kind === filter);
    else src = this.osc.filter((r) => r.kind === filter);
    return q ? src.filter((r) => `${r.addr} ${r.payload}`.toLowerCase().includes(q)) : src;
  }
}

function mergeById(a: LogRow[], b: LogRow[]): LogRow[] {
  const out: LogRow[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) out.push(a[i].id <= b[j].id ? a[i++] : b[j++]);
  while (i < a.length) out.push(a[i++]);
  while (j < b.length) out.push(b[j++]);
  return out;
}

const pad = (n: number, w = 2) => String(n).padStart(w, "0");

/** `HH:MM:SS.mmm` en hora local. */
export function formatClock(tMs: number): string {
  const d = new Date(tMs);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
}

/** Texto plano de las filas para el portapapeles (`×N` solo si se colapsó). */
export function rowsToText(rows: LogRow[]): string {
  return rows
    .map((r) => `${formatClock(r.tMs)}\t${r.kind}\t${r.addr}\t${r.payload}${r.count > 1 ? `\t×${r.count}` : ""}`)
    .join("\n");
}
