import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { defaults, phrases, type Settings, type Snapshot } from "./model";
import { preparePreview } from "./wallpaper";
import { phrasesFor } from "./i18n";

export const native = isTauri();
const saved = (() => {
  try {
    return JSON.parse(localStorage.getItem("leafshield-preview") || "null");
  } catch {
    return null;
  }
})();
const settings: Settings = { ...defaults, ...saved };
if (!["zh-CN", "en"].includes(settings.language)) settings.language = "zh-CN";
const phraseIndex = phrases.findIndex(
  (phrase, index) =>
    phrase === settings.phrase || phrasesFor("en")[index] === settings.phrase,
);
settings.phrase = phrasesFor(settings.language)[Math.max(0, phraseIndex)];
let state: Snapshot = {
  phase: "working",
  remainingSeconds: settings.workMinutes * 60,
  totalSeconds: settings.workMinutes * 60,
  settings,
  statistics: {
    date: new Date().toLocaleDateString(),
    focusSeconds: 0,
    restSeconds: 0,
    completedRests: 0,
    mindfulUnlocks: 0,
    emergencyUnlocks: 0,
  },
  logs: [],
  phrases: phrasesFor(settings.language),
  emergencyCount: 0,
  deferredBy: null,
  keyboardProtection: false,
  platform: "preview",
  notice: null,
  dataPath: "浏览器本地预览",
};
let emergencyLast = 0,
  emergencyFirst = 0;
const handlers = new Set<(value: Snapshot) => void>();
const emit = () => {
  for (const handler of handlers) handler(structuredClone(state));
};
const reset = () => {
  state.phase = "working";
  state.totalSeconds = state.settings.workMinutes * 60;
  state.remainingSeconds = state.totalSeconds;
  state.emergencyCount = 0;
};
const log = (kind: string, detail: string) => {
  state.logs.unshift({ at: new Date().toISOString(), kind, detail });
  state.logs = state.logs.slice(0, 200);
};
export async function call<T = Snapshot>(
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (native) return invoke<T>(command, args);
  if (command === "preview_display") return undefined as T;
  if (command === "get_startup" || command === "set_startup") {
    if (command === "set_startup")
      localStorage.setItem(
        "leafshield-preview-startup",
        String(args.enabled === true),
      );
    return {
      supported: true,
      enabled: localStorage.getItem("leafshield-preview-startup") !== "false",
      pathMatches: true,
      executable: "",
    } as T;
  } else if (command === "get_custom_wallpaper") {
    const stored = JSON.parse(
      localStorage.getItem("leafshield-wallpaper") || "null",
    );
    return (
      stored?.name === state.settings.customWallpaper ? stored.dataUrl : null
    ) as T;
  } else if (command === "set_custom_wallpaper") {
    if (state.phase === "resting") throw new Error("休息结束后可以修改设置");
    const stored = await preparePreview(String(args.data));
    localStorage.setItem("leafshield-wallpaper", JSON.stringify(stored));
    state.settings.customWallpaper = stored.name;
    state.settings.theme = "custom";
    localStorage.setItem("leafshield-preview", JSON.stringify(state.settings));
  } else if (command === "save_settings") {
    if (state.phase === "resting") throw new Error("休息结束后可以修改设置");
    const next = args.settings as Settings;
    if (next.language !== state.settings.language) {
      const index = state.phrases.indexOf(next.phrase);
      if (index >= 0) next.phrase = phrasesFor(next.language)[index];
    }
    if (
      next.workMinutes < 1 ||
      next.workMinutes > 240 ||
      next.restMinutes < 1 ||
      next.restMinutes > 60 ||
      !["zh-CN", "en"].includes(next.language) ||
      !phrasesFor(next.language).includes(next.phrase)
    )
      throw new Error("请检查时长，解锁文案只能选择 5 条预设");
    const resetDuration = next.workMinutes !== state.settings.workMinutes;
    state.settings = structuredClone(next);
    state.phrases = phrasesFor(next.language);
    localStorage.setItem("leafshield-preview", JSON.stringify(next));
    if (resetDuration) reset();
  } else if (command === "toggle_pause") {
    if (state.phase === "resting") throw new Error("请完成当前休息");
    state.phase = state.phase === "paused" ? "working" : "paused";
  } else if (command === "start_rest") {
    state.phase = "resting";
    state.remainingSeconds = state.settings.restMinutes * 60;
    state.totalSeconds = state.remainingSeconds;
    state.emergencyCount = 0;
  } else if (command === "unlock") {
    if (state.phase !== "resting" || args.text !== state.settings.phrase)
      throw new Error("请准确输入完整文案");
    state.statistics.mindfulUnlocks++;
    log("mindful", "手动输入正念文案，提前结束休息");
    reset();
  } else if (command === "emergency_key") {
    if (state.phase !== "resting" || !state.settings.emergencyEnabled)
      throw new Error("紧急通道当前不可用");
    const now = performance.now();
    if (now - emergencyLast > 2000 || now - emergencyFirst > 20000) {
      state.emergencyCount = 0;
      emergencyFirst = now;
    }
    if (state.emergencyCount === 0) emergencyFirst = now;
    if (now - emergencyLast >= 35) state.emergencyCount++;
    emergencyLast = now;
    if (state.emergencyCount >= 50) {
      state.statistics.emergencyUnlocks++;
      log("emergency", "紧急解锁：连续手动敲击 50 次");
      reset();
    }
  } else if (command === "reset_widget") {
    state.settings.floating = true;
    localStorage.setItem("leafshield-preview", JSON.stringify(state.settings));
  } else if (command === "preview_rest") {
    window.dispatchEvent(new CustomEvent("preview-rest"));
  } else if (command === "window_action") {
    window.dispatchEvent(new CustomEvent("close-preview"));
  }
  emit();
  return structuredClone(state) as T;
}
export async function subscribe(handler: (snapshot: Snapshot) => void) {
  if (native)
    return listen<Snapshot>("timer-update", (event) => handler(event.payload));
  handlers.add(handler);
  return () => {
    handlers.delete(handler);
  };
}
export interface WidgetDock {
  edge: "left" | "right" | "top" | "bottom" | null;
  hidden: boolean;
}
export async function subscribeDock(handler: (state: WidgetDock) => void) {
  if (native)
    return listen<WidgetDock>("widget-dock", (event) => handler(event.payload));
  const listener = (event: Event) =>
    handler((event as CustomEvent<WidgetDock>).detail);
  window.addEventListener("preview-widget-dock", listener);
  return () => window.removeEventListener("preview-widget-dock", listener);
}
if (!native)
  setInterval(() => {
    if (state.phase === "working" || state.phase === "resting") {
      state.remainingSeconds = Math.max(0, state.remainingSeconds - 1);
      if (state.phase === "working") state.statistics.focusSeconds++;
      else state.statistics.restSeconds++;
      if (state.remainingSeconds === 0) {
        if (state.phase === "working") {
          state.phase = "resting";
          state.remainingSeconds = state.settings.restMinutes * 60;
          state.totalSeconds = state.remainingSeconds;
        } else {
          state.statistics.completedRests++;
          log("completed", "完成了一次完整休息");
          reset();
        }
      }
    }
    if (state.emergencyCount && performance.now() - emergencyLast > 2000)
      state.emergencyCount = 0;
    emit();
  }, 1000);
