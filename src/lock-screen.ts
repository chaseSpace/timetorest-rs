import { html, tr } from "./i18n";
import "./lock-screen.css";
import { cursorPosition, getCurrentWindow } from "@tauri-apps/api/window";
import { call, native } from "./bridge";
import { escapeHtml as esc, time, type Snapshot } from "./model";
import { photo } from "./wallpaper";

type Act = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<boolean>;

export class LockScreen {
  private events = new AbortController();
  private emergencyOpen = false;
  private imageKey = "";
  private disposed = false;
  constructor(
    private root: HTMLElement,
    initial: Snapshot,
    private preview: boolean,
    private act: Act,
    private close: () => void,
    private toast: (message: string, error?: boolean) => void,
  ) {
    const s = initial.settings;
    document.body.className = "lock-body";
    root.innerHTML = html(
      `<section class="lock-screen" aria-label="休息锁屏"><img class="lock-photo" alt=""/><div class="lock-shade"></div><header class="lock-header"><span>一叶 <small>LEAFSHIELD</small></span>${preview ? '<button id="close-preview">关闭预览 ×</button>' : '<span class="lock-status">休息时间</span>'}</header><main class="lock-content"><h1>休息一下</h1><div class="lock-timer" data-rest-timer></div><p class="lock-hint">看看远处，放松双眼。</p><section class="lock-unlock" aria-label="提前结束休息"><p class="lock-instruction">需要提前回来？请手动输入以下文案</p><blockquote id="unlock-phrase">${esc(s.phrase)}</blockquote><label class="sr-only" for="unlock-input">手动输入正念文案</label><input id="unlock-input" type="text" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="逐字输入，不支持粘贴" maxlength="240"/><div class="lock-actions"><span id="unlock-progress">0 / ${[...s.phrase].length} 字</span><button id="unlock-button" disabled>正念解锁</button></div></section>${s.emergencyEnabled ? '<button class="lock-emergency-toggle" id="emergency-toggle" aria-expanded="false">突发紧急情况？</button><div class="lock-emergency" id="emergency-panel" hidden><p>20 秒内手动敲击 50 次；间隔超过 2 秒重新计数，长按无效。解锁会记录日志。</p><progress id="emergency-fill" value="0" max="50"></progress><b><span id="emergency-count">0</span> / 50</b></div>' : ""}</main><footer class="lock-footer"><span>${preview ? "预览不会锁定桌面或记录解锁" : "休息结束后自动返回"}</span><span id="photo-credit"></span></footer></section>`,
    );
    const input = root.querySelector<HTMLInputElement>("#unlock-input")!;
    const button = root.querySelector<HTMLButtonElement>("#unlock-button")!;
    const unlockPanel = root.querySelector<HTMLElement>(".lock-unlock")!;
    const progress = root.querySelector<HTMLElement>("#unlock-progress")!;
    const options = { signal: this.events.signal };
    unlockPanel.classList.add("is-idle");
    input.setAttribute("aria-describedby", "unlock-progress");
    const focusInput = () => {
      if (
        !this.disposed &&
        document.hasFocus() &&
        document.activeElement !== input
      )
        input.focus({ preventScroll: true });
    };
    let focusing = false;
    const revealUnlock = (activateWindow = false) => {
      if (this.disposed) return;
      unlockPanel.classList.remove("is-idle");
      focusInput();
      if (activateWindow && native && !focusing) {
        focusing = true;
        void call<void>("focus_rest_input")
          .then(focusInput)
          .catch((error) => {
            if (!this.disposed) this.toast(String(error), true);
          })
          .finally(() => {
            focusing = false;
          });
      }
    };
    // WebView2 can emit a mousemove when a full-screen window is positioned.
    // Compare physical desktop coordinates instead of its movementX/Y values.
    const initialPointer = native ? cursorPosition().catch(() => null) : null;
    const nativeWindow = native ? getCurrentWindow() : null;
    let lastPointer: { x: number; y: number } | null = null;
    let samplingPointer = false;
    window.addEventListener(
      "mousemove",
      async (event) => {
        const previous = lastPointer;
        lastPointer = { x: event.screenX, y: event.screenY };
        if (
          !event.isTrusted ||
          this.disposed ||
          samplingPointer ||
          (!unlockPanel.classList.contains("is-idle") && document.hasFocus())
        )
          return;
        if (nativeWindow) {
          samplingPointer = true;
          try {
            const [initial, current, origin, size] = await Promise.all([
              initialPointer,
              cursorPosition(),
              nativeWindow.innerPosition(),
              nativeWindow.innerSize(),
            ]);
            if (initial) {
              if (
                (current.x !== initial.x || current.y !== initial.y) &&
                current.x >= origin.x &&
                current.x < origin.x + size.width &&
                current.y >= origin.y &&
                current.y < origin.y + size.height
              )
                revealUnlock(true);
              return;
            }
          } catch {
            // Fall back to successive screen-coordinate samples.
          } finally {
            samplingPointer = false;
          }
        }
        if (
          previous &&
          (previous.x !== event.screenX || previous.y !== event.screenY)
        )
          revealUnlock(true);
      },
      options,
    );
    window.addEventListener(
      "keydown",
      (event) => {
        if (event.isTrusted && unlockPanel.classList.contains("is-idle"))
          revealUnlock();
      },
      { ...options, capture: true },
    );
    window.addEventListener("focus", focusInput, options);
    window.addEventListener(
      "blur",
      () => unlockPanel.classList.add("is-idle"),
      options,
    );
    for (const event of [
      "copy",
      "cut",
      "contextmenu",
      "dragstart",
      "selectstart",
    ])
      root.addEventListener(
        event,
        (e) => {
          if (e.target !== input) e.preventDefault();
        },
        options,
      );
    let composing = false;
    const validate = () => {
      const mismatch =
        !composing &&
        input.value.length > 0 &&
        !s.phrase.startsWith(input.value);
      button.disabled = composing || input.value !== s.phrase;
      input.setAttribute("aria-invalid", String(mismatch));
      progress.classList.toggle("is-mismatch", mismatch);
      progress.textContent = tr(
        `${[...input.value].length} / ${[...s.phrase].length} 字${mismatch ? " · 文案不匹配" : ""}`,
      );
    };
    input.addEventListener(
      "compositionstart",
      () => {
        revealUnlock();
        composing = true;
        validate();
      },
      options,
    );
    input.addEventListener(
      "compositionend",
      () => {
        composing = false;
        validate();
      },
      options,
    );
    input.addEventListener("input", validate, options);
    for (const event of ["paste", "drop", "contextmenu"])
      input.addEventListener(
        event,
        (e) => {
          e.preventDefault();
          if (event !== "contextmenu") toast("请逐字手动输入");
        },
        options,
      );
    input.addEventListener(
      "beforeinput",
      (e) => {
        if (
          ["insertFromPaste", "insertFromDrop", "insertFromYank"].includes(
            (e as InputEvent).inputType,
          )
        )
          e.preventDefault();
      },
      options,
    );
    input.addEventListener(
      "keydown",
      (e) => {
        if (
          ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "v") ||
          (e.shiftKey && e.key === "Insert")
        ) {
          e.preventDefault();
          toast("请逐字手动输入");
        }
        if (e.key === "Enter" && !composing && !button.disabled) button.click();
      },
      options,
    );
    button.onclick = () => {
      if (preview) close();
      else void act("unlock", { text: input.value });
    };
    const closeButton = root.querySelector<HTMLButtonElement>("#close-preview");
    if (closeButton) closeButton.onclick = close;
    const emergency =
      root.querySelector<HTMLButtonElement>("#emergency-toggle");
    if (emergency)
      emergency.onclick = () => {
        this.emergencyOpen = !this.emergencyOpen;
        root.querySelector<HTMLElement>("#emergency-panel")!.hidden =
          !this.emergencyOpen;
        emergency.setAttribute("aria-expanded", String(this.emergencyOpen));
      };
    window.addEventListener(
      "keydown",
      (event) => {
        if (
          !this.emergencyOpen ||
          event.repeat ||
          !event.isTrusted ||
          event.isComposing ||
          [
            "Shift",
            "Control",
            "Alt",
            "Meta",
            "CapsLock",
            "Tab",
            "Escape",
            "Enter",
          ].includes(event.key)
        )
          return;
        if (preview) {
          toast("预览中的紧急通道不会实际解锁");
          return;
        }
        event.preventDefault();
        void act("emergency_key");
      },
      { ...options, capture: true },
    );
    this.update(initial);
    // Only the active screen primes its field for the first key and IME input.
    // Background lock windows must not compete for keyboard focus.
    if (document.hasFocus()) focusInput();
  }
  update(snapshot: Snapshot) {
    this.root.querySelector("[data-rest-timer]")!.textContent = tr(
      time(
        this.preview
          ? snapshot.settings.restMinutes * 60
          : snapshot.remainingSeconds,
      ),
    );
    const count = this.root.querySelector("#emergency-count");
    if (count) count.textContent = tr(String(snapshot.emergencyCount));
    const meter =
      this.root.querySelector<HTMLProgressElement>("#emergency-fill");
    if (meter) meter.value = snapshot.emergencyCount;
    const key =
      snapshot.settings.theme + ":" + snapshot.settings.customWallpaper;
    if (key !== this.imageKey) {
      this.imageKey = key;
      void this.loadPhoto(snapshot, key);
    }
  }
  private async loadPhoto(snapshot: Snapshot, key: string) {
    const fallback = photo(snapshot.settings);
    let src = fallback.src;
    let credit = fallback.credit;
    if (snapshot.settings.theme === "custom") {
      try {
        const custom = await call<string | null>("get_custom_wallpaper");
        if (!custom) throw new Error("自定义图片已丢失，暂时使用林间背景");
        src = custom;
        credit = "自定义背景";
      } catch (error) {
        this.toast(String(error).replace(/^Error: /, ""), true);
      }
    }
    if (this.disposed || key !== this.imageKey) return;
    const image = this.root.querySelector<HTMLImageElement>(".lock-photo")!;
    image.onerror = () => {
      image.onerror = null;
      image.src = "/wallpapers/forest.jpg";
      this.root.querySelector("#photo-credit")!.textContent =
        tr("Ingmar / Unsplash");
    };
    image.src = src;
    this.root.querySelector("#photo-credit")!.textContent = tr(credit);
  }
  destroy() {
    this.disposed = true;
    this.events.abort();
  }
}
