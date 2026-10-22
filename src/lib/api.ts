import { invoke } from "@tauri-apps/api/core";

export type Look = {
  zoom: number | null;
  exposure: number | null;
  focus: "auto" | "semi" | "manual" | null;
  focalDistance: number | null;
  aperture: number | null;
};

export type Preset = {
  name: string;
  category: "retrato" | "cuerpo" | "ambiente" | "movimiento" | "video" | "ref_sheet";
  modifier: boolean;
  frame: object | null;
  look: Look;
};

export type Motion = {
  name: string;
  duration: number;
  loop: "once" | "repeat" | "pingPong";
  handheld: boolean;
};

export type MotionStatus = { name: string | null; state: "playing" | "paused" | "stopped"; error: string | null };
export type MotionCmd = "play" | "pause" | "reverse" | { loop: boolean } | { speed: number };
export type TurnaroundProgress = { done: number; total: number; finished: boolean; error: string | null };

export type Status = { connected: boolean; open: boolean; anchored: boolean };

export const api = {
  connect: () => invoke<void>("connect"),
  status: () => invoke<Status>("status"),
  calibrate: () => invoke<void>("calibrate"),
  resetDefaults: () => invoke<void>("reset_defaults"),
  listPresets: () => invoke<{ presets: Preset[]; warnings: string[] }>("list_presets"),
  applyPreset: (name: string) => invoke<void>("apply_preset", { name }),
  listMotions: () => invoke<Motion[]>("list_motions"),
  playMotion: (name: string) => invoke<void>("play_motion", { name }),
  motionControl: (cmd: MotionCmd) => invoke<void>("motion_control", { cmd }),
  motionStatus: () => invoke<MotionStatus>("motion_status"),
  stopMotion: () => invoke<void>("stop_motion"),
  panicStop: () => invoke<void>("panic_stop"),
  startTurnaround: (fourViews: boolean) => invoke<void>("start_turnaround", { fourViews }),
  turnaroundStatus: () => invoke<TurnaroundProgress | null>("turnaround_status"),
};

/** Focal equivalente full-frame si Zoom es FOV vertical (aproximado hasta medirlo, ver docs/osc-findings.md). */
export const approxMm = (fov: number) => Math.round(12 / Math.tan((fov * Math.PI) / 360));
