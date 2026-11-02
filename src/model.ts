export type Phase = "working" | "paused" | "resting" | "deferred";
export interface StartupInfo {
  supported: boolean;
  enabled: boolean;
  pathMatches: boolean;
  executable: string;
}
import type { Language } from "./i18n";
export interface Settings {
  language: Language;
  workMinutes: number;
  restMinutes: number;
  phrase: string;
  theme: string;
  temperatureKelvin: number;
  brightnessPercent: number;
  displayEnabled: boolean;
  customWallpaper: string | null;
  floating: boolean;
  clickThrough: boolean;
  edgeHide: boolean;
  notifications: boolean;
  emergencyEnabled: boolean;
  whitelist: string[];
}
export interface LogEntry {
  at: string;
  kind: string;
  detail: string;
}
export interface Snapshot {
  phase: Phase;
  remainingSeconds: number;
  totalSeconds: number;
  settings: Settings;
  statistics: {
    date: string;
    focusSeconds: number;
    restSeconds: number;
    completedRests: number;
    mindfulUnlocks: number;
    emergencyUnlocks: number;
  };
  logs: LogEntry[];
  phrases: string[];
  emergencyCount: number;
  deferredBy: string | null;
  keyboardProtection: boolean;
  platform: string;
  notice: string | null;
  dataPath: string;
  smokeMode?: boolean;
  displayStatus?: {
    supported: boolean;
    active: boolean;
    monitors: number;
    error: string | null;
    backend: string;
    rgb: [number, number, number];
    transitioning: boolean;
    updates: number;
  };
}
export const phrases = [
  "我知道休息不足可能会伤害眼睛和大脑，继续解锁",
  "工作可以稍等片刻，我的眼睛值得被认真照顾",
  "我选择有意识地继续工作，并记得稍后好好休息",
  "停下来不是浪费时间，而是为了更清醒地出发",
  "此刻我愿意放慢脚步，给眼睛和身体一点温柔",
];
export const defaults: Settings = {
  language: "zh-CN",
  workMinutes: 45,
  restMinutes: 2,
  phrase: phrases[0],
  theme: "forest",
  temperatureKelvin: 5000,
  brightnessPercent: 80,
  displayEnabled: true,
  customWallpaper: null,
  floating: true,
  clickThrough: false,
  edgeHide: true,
  notifications: true,
  emergencyEnabled: true,
  whitelist: [],
};
export const time = (seconds: number) =>
  `${Math.floor(seconds / 60)
    .toString()
    .padStart(2, "0")}:${Math.max(0, seconds % 60)
    .toString()
    .padStart(2, "0")}`;
export const escapeHtml = (value: string) =>
  value.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ]!,
  );
