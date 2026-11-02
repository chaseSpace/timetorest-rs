import { html, tr, setLocale, localizeTree } from "./i18n";
import "./compact.css";
import { LockScreen } from "./lock-screen";
import { call, native, subscribe } from "./bridge";
import { CompactUI } from "./compact";
import { renderWidget } from "./widget";
import { escapeHtml as esc, time, type Snapshot } from "./model";

const icons: Record<string, string> = {
  leaf: '<path d="M19 4C8 3 3 8 5 14c2 6 9 6 12 1 2-3 1-7 2-11Z"/><path d="M5 21c2-7 6-11 11-14"/>',
  home: '<rect x="4" y="4" width="6" height="6" rx="1.5"/><rect x="14" y="4" width="6" height="6" rx="1.5"/><rect x="4" y="14" width="6" height="6" rx="1.5"/><rect x="14" y="14" width="6" height="6" rx="1.5"/>',
  settings:
    '<path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="3"/><circle cx="15" cy="17" r="3"/>',
  history: '<path d="M4 10a8 8 0 1 1 1 8M4 4v6h6M12 7v5l3 2"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7v1"/>',
  play: '<path d="m9 5 10 7-10 7Z"/>',
  pause: '<path d="M8 5v14M16 5v14"/>',
  arrow: '<path d="M5 12h14m-5-5 5 5-5 5"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1"/>',
  check: '<path d="m5 12 4 4L19 6"/>',
  lock: '<rect x="5" y="10" width="14" height="11" rx="3"/><path d="M8 10V7a4 4 0 0 1 8 0v3m-4 4v3"/>',
  close: '<path d="m6 6 12 12M6 18 18 6"/>',
  minus: '<path d="M5 12h14"/>',
  eye: '<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>',
  monitor:
    '<rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4"/>',
};
const icon = (name: string, cls = "") =>
  `<svg class="icon ${cls}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${icons[name] || icons.leaf}</svg>`;
const app = document.querySelector<HTMLDivElement>("#app")!;
let snapshot: Snapshot;
let compact: CompactUI | null = null;
let preview = false;
let lock: LockScreen | null = null;
let toastTimer: ReturnType<typeof setTimeout>;
const route = location.hash.slice(1);
const phaseText = (s: Snapshot) =>
  ({
    working: "专注进行中",
    paused: "计时已暂停",
    resting: "安心休息中",
    deferred: "会议保护中",
  })[s.phase];
const toast = (message: string, error = false) => {
  let element = document.querySelector<HTMLElement>("#toast");
  if (!element) {
    element = document.createElement("div");
    element.id = "toast";
    element.setAttribute("role", "status");
    document.body.append(element);
  }
  element.textContent = tr(message);
  element.className = `toast visible ${error ? "error" : ""}`;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => element?.classList.remove("visible"), 4200);
};
async function act(command: string, args: Record<string, unknown> = {}) {
  try {
    const result = await call(command, args);
    if (result && typeof result === "object" && "phase" in result)
      receive(result);
    return true;
  } catch (error) {
    toast(String(error).replace(/^Error: /, ""), true);
    return false;
  }
}
function shell() {
  lock?.destroy();
  lock = null;
  document.body.className = "";
  compact = new CompactUI(app, () => snapshot, act, icon, native);
  compact.render();
}
function renderRest(isPreview: boolean) {
  compact = null;
  lock?.destroy();
  preview = isPreview;
  lock = new LockScreen(app, snapshot, isPreview, act, closePreview, toast);
}
function closePreview() {
  if (native && route === "preview")
    void act("window_action", { action: "close" });
  else {
    preview = false;
    shell();
  }
}
function updateLive() {
  if (!snapshot) return;
  document
    .querySelectorAll("[data-timer]")
    .forEach((el) => (el.textContent = tr(time(snapshot.remainingSeconds))));
  document
    .querySelectorAll("[data-phase]")
    .forEach((el) => (el.textContent = tr(phaseText(snapshot))));
  document
    .querySelectorAll("[data-work-minutes]")
    .forEach(
      (el) => (el.textContent = tr(String(snapshot.settings.workMinutes))),
    );
  document
    .querySelectorAll("[data-rest-minutes]")
    .forEach(
      (el) => (el.textContent = tr(String(snapshot.settings.restMinutes))),
    );
  document
    .querySelectorAll("[data-stat]")
    .forEach(
      (el) =>
        (el.textContent = tr(
          String(
            (el as HTMLElement).dataset.stat === "focus"
              ? Math.floor(snapshot.statistics.focusSeconds / 60)
              : (el as HTMLElement).dataset.stat === "rest"
                ? snapshot.statistics.completedRests
                : Math.floor(snapshot.statistics.restSeconds / 60),
          ),
        )),
    );
  const pause = document.querySelector("#pause-text");
  if (pause)
    pause.textContent = tr(
      snapshot.phase === "paused" ? "继续专注" : "暂停计时",
    );
  const pauseButton = document.querySelector<HTMLButtonElement>("#pause-timer");
  if (pauseButton)
    pauseButton.disabled = !["working", "paused"].includes(snapshot.phase);
  const notice = document.querySelector<HTMLElement>("#notice");
  if (notice) {
    const text =
      snapshot.notice ||
      (snapshot.deferredBy
        ? `正在使用 ${snapshot.deferredBy}，切换到其他程序后开始休息。`
        : "");
    notice.textContent = tr(text);
    notice.hidden = !text;
  }
  lock?.update(snapshot);
}
function receive(value: Snapshot) {
  const languageChanged =
    snapshot?.settings.language !== value.settings.language;
  snapshot = value;
  setLocale(value.settings.language);
  if (languageChanged) {
    if (compact) compact.render();
    else if (lock) renderRest(preview);
    else localizeTree(app);
  }
  if (
    !native &&
    !preview &&
    route !== "widget" &&
    route !== "warning" &&
    route !== "preview"
  ) {
    if (snapshot.phase === "resting" && !lock) renderRest(false);
    else if (snapshot.phase !== "resting" && lock) {
      shell();
    }
  }
  compact?.update();
  updateLive();
}
window.addEventListener("preview-rest", () => renderRest(true));
window.addEventListener("close-preview", () => {
  if (preview) closePreview();
});
async function boot() {
  try {
    snapshot = await call("get_state");
    setLocale(snapshot.settings.language);
    if (route === "widget") {
      document.body.className = "widget-body";
      await renderWidget(app, icon, act);
    } else if (route === "warning") {
      document.body.className = "warning-body";
      app.innerHTML = html(
        `<div class="warning-card"><span class="widget-leaf">${icon("leaf")}</span><div><b>给手边的工作，留个结尾。</b><p><span data-timer></span> 后开始休息，记得保存。</p></div><button id="dismiss-warning" aria-label="收起提醒">${icon("close")}</button></div>`,
      );
      document.querySelector<HTMLButtonElement>("#dismiss-warning")!.onclick =
        () => {
          void act("window_action", { action: "close" });
        };
    } else if (route === "rest" || route === "preview")
      renderRest(route === "preview");
    else shell();
    updateLive();
    await subscribe(receive);
    if (native && snapshot.smokeMode && !route)
      await call("smoke_ready", {
        title: document.querySelector("h1")?.textContent || "",
      });
  } catch (error) {
    app.innerHTML = html(
      `<div class="startup-error"><img src="/leaf.svg" width="56"/><h1>一叶暂时没能启动</h1><p>${esc(String(error))}</p><button class="primary" id="retry">重新连接</button></div>`,
    );
    document.querySelector<HTMLButtonElement>("#retry")!.onclick = () =>
      location.reload();
  }
}
void boot();
