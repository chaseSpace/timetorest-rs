import { call, native, subscribeDock, type WidgetDock } from "./bridge";
export async function renderWidget(
  root: HTMLElement,
  icon: (name: string) => string,
  act: (command: string, args?: Record<string, unknown>) => Promise<boolean>,
) {
  root.innerHTML = `<div class="floating-widget"><div class="widget-expanded"><span class="widget-leaf">${icon("leaf")}</span><div class="widget-time"><small data-phase></small><b data-timer></b></div><button id="open-main" aria-label="打开一叶">${icon("arrow")}</button></div><span class="dock-tab" title="靠近展开悬浮计时" aria-label="展开悬浮计时"></span></div>`;
  root.querySelector<HTMLButtonElement>("#open-main")!.onclick = () => {
    void act("window_action", { action: "show" });
  };
  root.querySelector<HTMLElement>(".floating-widget")!.onpointerdown = (
    event,
  ) => {
    if (
      native &&
      event.button === 0 &&
      !(event.target as HTMLElement).closest("button")
    ) {
      event.preventDefault();
      void act("widget_drag");
    }
  };
  const apply = (state: WidgetDock) => {
    document.body.classList.toggle("widget-collapsed", state.hidden);
    document.body.dataset.dock = state.edge || "";
  };
  await subscribeDock(apply);
  if (native) apply(await call<WidgetDock>("get_widget_state"));
}
