import { html, tr, getLocale, phrasesFor, type Language } from "./i18n";
import {
  escapeHtml as esc,
  time,
  type Settings,
  type Snapshot,
  type StartupInfo,
} from "./model";
import { call } from "./bridge";
import { mergeWhitelist } from "./whitelist";
import { backgrounds, base64, checkUpload } from "./wallpaper";
type Icon = (name: string, cls?: string) => string;
type Act = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<boolean>;
type Page =
  "display" | "timer" | "widget" | "phrases" | "options" | "about" | "history";

export class CompactUI {
  private page: Page = "display";
  private displaySave: ReturnType<typeof setTimeout> | undefined;
  private displayWork = Promise.resolve();
  private displayPending: {
    temperatureKelvin: number;
    brightnessPercent: number;
    enabled: boolean;
  } | null = null;
  private queue = Promise.resolve();
  private logCount = -1;
  private startup: StartupInfo | null = null;
  constructor(
    private root: HTMLElement,
    private read: () => Snapshot,
    private act: Act,
    private icon: Icon,
    private native: boolean,
  ) {}
  render() {
    this.root.innerHTML = html(
      `<div class="utility-window"><header class="utility-titlebar" data-tauri-drag-region><h1 data-tauri-drag-region>${this.icon("leaf")}一叶 · LeafShield</h1><div class="window-controls"><button id="minimize" aria-label="最小化">${this.icon("minus")}</button><button id="hide-window" aria-label="收起到托盘" title="收起到系统托盘">${this.icon("close")}</button></div></header><div class="utility-body"><nav class="utility-nav" aria-label="主导航">${[
        ["display", "显示器"],
        ["timer", "计时器"],
        ["widget", "悬浮窗"],
        ["phrases", "解锁文案"],
        ["options", "选项"],
        ["about", "关于"],
      ]
        .map(
          ([id, label]) =>
            `<button data-page="${id}" class="${id === this.page ? "selected" : ""}">${label}</button>`,
        )
        .join(
          "",
        )}<span class="nav-sprout" aria-hidden="true">${this.icon("leaf")}</span></nav><main class="utility-content"><section id="utility-panel"></section><div class="utility-status" role="status" id="save-result">${this.native ? "设置自动保存" : "本地界面预览，不会锁定桌面"}</div></main></div></div>`,
    );
    this.root
      .querySelectorAll<HTMLButtonElement>("[data-page]")
      .forEach(
        (button) =>
          (button.onclick = () => this.navigate(button.dataset.page as Page)),
      );
    this.button("minimize", () => {
      void this.act("window_action", { action: "minimize" });
    });
    this.button("hide-window", () => {
      if (this.native) void this.act("window_action", { action: "hide" });
    });
    this.renderPanel();
    this.update();
  }
  private button(id: string, action: () => void) {
    const button = this.root.querySelector<HTMLButtonElement>("#" + id);
    if (button) button.onclick = action;
  }
  private navigate(page: Page) {
    this.page = page;
    this.root
      .querySelectorAll<HTMLElement>("[data-page]")
      .forEach((button) =>
        button.classList.toggle(
          "selected",
          button.dataset.page === (page === "history" ? "about" : page),
        ),
      );
    this.renderPanel();
    this.update();
  }
  private patch(patch: Partial<Settings>) {
    if (patch.language && patch.language !== this.read().settings.language) {
      const index = Math.max(
        0,
        this.read().phrases.indexOf(this.read().settings.phrase),
      );
      patch.phrase = phrasesFor(patch.language)[index];
    }
    this.queue = this.queue.then(async () => {
      const status = this.root.querySelector("#save-result");
      if (status) status.textContent = tr("正在保存…");
      const okay = await this.act("save_settings", {
        settings: { ...this.read().settings, ...patch },
      });
      if (status)
        status.textContent = tr(okay ? "设置已保存" : "保存失败，请重试");
      if (!okay) this.renderPanel();
      this.update();
    });
    return this.queue;
  }
  private displayValues() {
    return {
      temperatureKelvin: Number(
        this.root.querySelector<HTMLInputElement>("#temperature")!.value,
      ),
      brightnessPercent: Number(
        this.root.querySelector<HTMLInputElement>("#brightness")!.value,
      ),
      enabled:
        this.root.querySelector<HTMLInputElement>("#displayEnabled")!.checked,
    };
  }
  private commitDisplay() {
    clearTimeout(this.displaySave);
    this.displaySave = undefined;
    this.displayPending = null;
    const { temperatureKelvin, brightnessPercent, enabled } =
      this.displayValues();
    this.displayWork = this.displayWork.then(() =>
      this.patch({
        temperatureKelvin,
        brightnessPercent,
        displayEnabled: enabled,
      }),
    );
  }
  private toggle(id: keyof Settings, label: string, hint = "") {
    return `<label class="utility-toggle" for="${id}"><span>${label}${hint ? `<small>${hint}</small>` : ""}</span><input id="${id}" type="checkbox" ${this.read().settings[id] ? "checked" : ""}/><span class="utility-switch" aria-hidden="true"></span></label>`;
  }
  private chooseWallpaper(file: File) {
    this.queue = this.queue.then(async () => {
      const status = this.root.querySelector("#save-result");
      const button =
        this.root.querySelector<HTMLButtonElement>("#choose-wallpaper");
      if (button) button.disabled = true;
      try {
        checkUpload(file);
        if (status) status.textContent = tr("正在压缩并保存图片…");
        const okay = await this.act("set_custom_wallpaper", {
          data: await base64(file),
        });
        if (status)
          status.textContent = tr(
            okay ? "自定义背景已保存" : "图片保存失败，请重试",
          );
      } catch (error) {
        if (status)
          status.textContent = tr(String(error).replace(/^Error: /, ""));
      } finally {
        if (button) button.disabled = false;
        this.update();
      }
    });
  }
  private async readStartup() {
    try {
      this.startup = await call<StartupInfo>("get_startup");
      if (this.page === "options") this.updateStartup();
    } catch (error) {
      const hint = this.root.querySelector("#startup-hint");
      if (hint) hint.textContent = tr(String(error).replace(/^Error: /, ""));
    }
  }
  private updateStartup() {
    const input = this.root.querySelector<HTMLInputElement>("#startup-enabled");
    const hint = this.root.querySelector("#startup-hint");
    if (!input || !hint || !this.startup) return;
    input.disabled = !this.startup.supported;
    input.checked = this.startup.enabled;
    hint.textContent = tr(
      !this.native
        ? "仅界面预览，不修改系统"
        : !this.startup.supported
          ? "当前仅支持 Windows"
          : this.startup.enabled && !this.startup.pathMatches
            ? "路径已改变，请关闭后重新开启"
            : "登录 Windows 后在托盘运行",
    );
  }
  private changeStartup(enabled: boolean) {
    this.queue = this.queue.then(async () => {
      const input =
        this.root.querySelector<HTMLInputElement>("#startup-enabled");
      const status = this.root.querySelector("#save-result");
      if (input) input.disabled = true;
      try {
        this.startup = await call<StartupInfo>("set_startup", { enabled });
        if (status)
          status.textContent = tr(
            enabled ? "已设置开机启动" : "已关闭开机启动",
          );
      } catch (error) {
        if (status)
          status.textContent = tr(String(error).replace(/^Error: /, ""));
        await this.readStartup();
      } finally {
        this.updateStartup();
      }
    });
  }
  private async choosePrograms() {
    if (!this.native) {
      this.root.querySelector<HTMLInputElement>("#whitelist-files")!.click();
      return;
    }
    const button =
      this.root.querySelector<HTMLButtonElement>("#choose-program");
    if (button) button.disabled = true;
    try {
      const names = await call<string[]>("pick_whitelist_programs");
      if (names.length) this.addPrograms(names);
    } catch (error) {
      const status = this.root.querySelector("#save-result");
      if (status) {
        status.textContent = tr(String(error).replace(/^Error: /, ""));
        (status as HTMLElement).title = status.textContent;
      }
    } finally {
      if (button) button.disabled = false;
    }
  }
  private addPrograms(names: string[]) {
    this.queue = this.queue.then(async () => {
      const input = this.root.querySelector<HTMLTextAreaElement>("#whitelist");
      const status = this.root.querySelector("#save-result");
      if (!input || !status) return;
      try {
        const whitelist = mergeWhitelist(input.value, names);
        const okay = await this.act("save_settings", {
          settings: { ...this.read().settings, whitelist },
        });
        if (okay) input.value = whitelist.join("\n");
        status.textContent = tr(okay ? "白名单已保存" : "保存失败，请重试");
      } catch (error) {
        status.textContent = tr(String(error).replace(/^Error: /, ""));
      }
    });
  }
  private bindToggle(id: keyof Settings) {
    const input = this.root.querySelector<HTMLInputElement>("#" + id);
    if (input) input.onchange = () => this.patch({ [id]: input.checked });
  }
  private renderPanel() {
    const s = this.read().settings;
    const panel = this.root.querySelector("#utility-panel")!;
    if (this.page === "display") {
      panel.innerHTML = html(
        `<div class="utility-slider display-slider"><div><label for="temperature">更温暖</label><output id="temperature-value">${s.temperatureKelvin}K</output><span>不太温暖</span></div><input id="temperature" type="range" min="1500" max="6500" step="100" value="${s.temperatureKelvin}" aria-label="屏幕色温"/></div><div class="utility-slider display-slider"><div><label for="brightness">不太明亮</label><output id="brightness-value">${s.brightnessPercent}%</output><span>更明亮</span></div><input id="brightness" type="range" min="20" max="100" value="${s.brightnessPercent}" aria-label="屏幕亮度"/></div>${this.toggle("displayEnabled", "启用屏幕调节")}<div class="utility-action"><button class="classic-button" id="restore-display">恢复原色</button><span id="display-status"></span></div><p class="utility-help">系统 RGB 色温与亮度调节，平滑过渡。<br/>退出后自动恢复原色。</p>`,
      );
      for (const [id, key, suffix] of [
        ["temperature", "temperatureKelvin", "K"],
        ["brightness", "brightnessPercent", "%"],
      ] as const) {
        const input = this.root.querySelector<HTMLInputElement>("#" + id)!;
        input.oninput = () => {
          this.root.querySelector("#" + id + "-value")!.textContent = tr(
            input.value + suffix,
          );
          this.displayPending = this.displayValues();
          if (!this.displaySave)
            this.displaySave = setTimeout(() => {
              this.displaySave = undefined;
              const values = this.displayPending;
              this.displayPending = null;
              if (values)
                this.displayWork = this.displayWork
                  .then(async () => {
                    await call<void>("preview_display", values);
                  })
                  .catch((error) => {
                    const status = this.root.querySelector("#save-result");
                    if (status)
                      status.textContent = tr(
                        String(error).replace(/^Error: /, ""),
                      );
                  });
            }, 50);
        };
        input.onchange = () => {
          this.commitDisplay();
        };
      }
      this.root.querySelector<HTMLInputElement>("#displayEnabled")!.onchange =
        () => this.commitDisplay();
      this.button("restore-display", () => {
        clearTimeout(this.displaySave);
        this.displaySave = undefined;
        this.displayPending = null;
        this.displayWork = this.displayWork.then(() =>
          this.patch({ temperatureKelvin: 6500, brightnessPercent: 100 }),
        );
      });
    } else if (this.page === "timer") {
      panel.innerHTML = html(
        `<div class="utility-slider"><div><label for="work-minutes">工作时长</label><output id="work-value">${s.workMinutes} 分钟</output><small>1–240 分钟</small></div><input id="work-minutes" type="range" min="1" max="240" value="${s.workMinutes}"/></div><div class="utility-slider"><div><label for="rest-minutes">休息时长</label><output id="rest-value">${s.restMinutes} 分钟</output><small>1–60 分钟</small></div><input id="rest-minutes" type="range" min="1" max="60" value="${s.restMinutes}"/></div><div class="utility-countdown"><span data-phase>专注进行中</span><strong data-timer>${time(this.read().remainingSeconds)}</strong><button id="pause-timer" class="classic-button"><span id="pause-text">暂停计时</span></button><button id="rest-now" class="classic-button">现在休息</button></div>${this.toggle("notifications", "休息前提醒", "提前 60 秒、30 秒轻声提醒")}<div class="rhythm-presets">${[
          [25, 2],
          [45, 2],
          [60, 3],
          [90, 5],
        ]
          .map(
            ([work, rest]) =>
              `<button class="classic-button" data-rhythm="${work},${rest}" title="工作 ${work} 分钟，休息 ${rest} 分钟">${work} / ${rest}</button>`,
          )
          .join(
            "",
          )}</div><p class="utility-help" id="rhythm-description">专注 ${s.workMinutes} 分钟，休息 ${s.restMinutes} 分钟。关闭窗口后在托盘继续计时。</p><p class="utility-notice" id="notice" hidden></p>`,
      );
      for (const [id, key, output] of [
        ["work-minutes", "workMinutes", "work-value"],
        ["rest-minutes", "restMinutes", "rest-value"],
      ] as const) {
        const input = this.root.querySelector<HTMLInputElement>("#" + id)!;
        input.oninput = () => {
          this.root.querySelector("#" + output)!.textContent = tr(
            input.value + " 分钟",
          );
        };
        input.onchange = () => this.patch({ [key]: Number(input.value) });
      }
      this.root.querySelectorAll<HTMLButtonElement>("[data-rhythm]").forEach(
        (button) =>
          (button.onclick = () => {
            const [workMinutes, restMinutes] = button.dataset
              .rhythm!.split(",")
              .map(Number);
            this.patch({ workMinutes, restMinutes });
          }),
      );
      this.button("pause-timer", () => {
        void this.act("toggle_pause");
      });
      this.button("rest-now", () => {
        void this.act("start_rest");
      });
      this.bindToggle("notifications");
    } else if (this.page === "widget") {
      panel.innerHTML = html(
        `<h2>桌面悬浮计时</h2>${this.toggle("floating", "显示悬浮窗")}${this.toggle("edgeHide", "贴边吸附后自动隐藏", "鼠标离开收起，靠近边条展开")}${this.toggle("clickThrough", "鼠标穿透", "展开时不拦截点击；可从托盘调整")}<div class="widget-example"><span>${this.icon("leaf")}<b>${time(this.read().remainingSeconds)}</b>${this.icon("arrow")}</span><small>拖动悬浮窗到屏幕任意边缘</small></div><div class="utility-action"><button id="reset-widget" class="classic-button">恢复悬浮窗位置</button><span>收起后会保留一条可唤出的边条。</span></div>`,
      );
      this.bindToggle("floating");
      this.bindToggle("edgeHide");
      this.bindToggle("clickThrough");
      this.button("reset-widget", () => {
        void this.act("reset_widget");
      });
    } else if (this.page === "phrases") {
      panel.innerHTML = html(
        `<h2>提前结束休息时的解锁文案</h2><p class="utility-help">仅提供以下 5 条预设。请在休息界面完整手动输入，不支持粘贴。</p><label class="preset-label" for="phrase-preset">选择文案</label><select id="phrase-preset" aria-label="选择解锁文案">${this.read()
          .phrases.map(
            (phrase, index) =>
              `<option value="${index}" ${phrase === s.phrase ? "selected" : ""}>${index + 1}. ${esc(phrase)}</option>`,
          )
          .join(
            "",
          )}</select><div class="phrase-sample"><small>当前文案</small><blockquote id="preset-preview">${esc(s.phrase)}</blockquote></div><p class="utility-help">完整休息结束后会自动解锁，无需输入文案。</p><button id="preview-rest" class="classic-button">预览休息界面</button>`,
      );
      this.root.querySelector<HTMLSelectElement>("#phrase-preset")!.onchange = (
        event,
      ) =>
        this.patch({
          phrase:
            this.read().phrases[
              Number((event.target as HTMLSelectElement).value)
            ],
        });
      this.button("preview-rest", () => {
        void this.act("preview_rest");
      });
    } else if (this.page === "options") {
      panel.innerHTML = html(
        `<div class="options-heading"><h2>休息选项</h2><label for="language">语言 <select id="language" aria-label="语言"><option value="zh-CN" data-no-i18n>简体中文</option><option value="en">English</option></select></label></div><div class="theme-row"><span>休息背景</span>${backgrounds.map(({ id, label }) => `<button class="classic-button" data-theme="${id}" aria-pressed="${s.theme === id}">${label}</button>`).join("")}<button class="classic-button" data-theme="custom" ${s.customWallpaper ? "" : "disabled"}>自定义</button><button id="preview-rest" class="classic-button">预览</button></div><div class="wallpaper-upload"><button id="choose-wallpaper" class="classic-button">选择图片</button><input id="wallpaper-file" type="file" accept="image/jpeg,image/png,image/webp" hidden/><span>JPEG / PNG / WebP，最大 5 MB<br/>自动缩放至 1920×1200 内，保存不超过 500 KB</span></div><label class="utility-toggle startup-option" for="startup-enabled"><span>开机启动<small id="startup-hint">正在读取启动项…</small></span><input id="startup-enabled" type="checkbox" disabled/><span class="utility-switch" aria-hidden="true"></span></label>${this.toggle("emergencyEnabled", "启用紧急解锁", "20 秒内敲击 50 次，解锁后记入日志")}<label class="whitelist-label" for="whitelist">前台程序白名单 <small>最多 5 个，每行一个进程名</small></label><textarea id="whitelist" rows="2" spellcheck="false" placeholder="ms-teams.exe&#10;Zoom.exe">${esc(s.whitelist.join("\n"))}</textarea><div class="utility-action"><button id="choose-program" class="classic-button" title="支持 EXE 和桌面快捷方式（.lnk）">选择程序</button><input id="whitelist-files" type="file" accept=".exe,.lnk" multiple hidden/><button id="save-whitelist" class="classic-button">保存白名单</button><span>这些程序在前台时延后休息。</span></div>`,
      );
      this.button("choose-wallpaper", () =>
        this.root.querySelector<HTMLInputElement>("#wallpaper-file")!.click(),
      );
      this.root.querySelector<HTMLInputElement>("#wallpaper-file")!.onchange = (
        event,
      ) => {
        const input = event.target as HTMLInputElement;
        const file = input.files?.[0];
        if (file) this.chooseWallpaper(file);
        input.value = "";
      };
      this.root
        .querySelectorAll<HTMLButtonElement>("[data-theme]")
        .forEach(
          (button) =>
            (button.onclick = () =>
              this.patch({ theme: button.dataset.theme! })),
        );
      const language = this.root.querySelector<HTMLSelectElement>("#language")!;
      language.value = s.language;
      language.onchange = () => {
        void this.patch({ language: language.value as Language });
      };
      this.root.querySelector<HTMLInputElement>("#startup-enabled")!.onchange =
        (event) =>
          this.changeStartup((event.target as HTMLInputElement).checked);
      void this.readStartup();
      this.bindToggle("emergencyEnabled");
      this.button("preview-rest", () => {
        void this.act("preview_rest");
      });
      this.button("choose-program", () => {
        void this.choosePrograms();
      });
      this.root.querySelector<HTMLInputElement>("#whitelist-files")!.onchange =
        (event) => {
          const input = event.target as HTMLInputElement;
          const files = Array.from(input.files || []);
          input.value = "";
          const status = this.root.querySelector("#save-result");
          if (files.some((file) => !file.name.toLowerCase().endsWith(".exe"))) {
            if (status)
              status.textContent = tr(
                "\u5feb\u6377\u65b9\u5f0f\u8bf7\u5728\u684c\u9762\u7248\u9009\u62e9\uff0c\u6d4f\u89c8\u5668\u9884\u89c8\u4ec5\u652f\u6301 EXE",
              );
            return;
          }
          if (files.length) this.addPrograms(files.map((file) => file.name));
        };
      this.button("save-whitelist", () => this.addPrograms([]));
    } else if (this.page === "about") {
      const state = this.read();
      panel.innerHTML = html(
        `<h2>一叶 · LeafShield <small>0.6.2</small></h2><p class="about-motto">定时休息，给眼睛一点留白。<span class="about-author">作者：磊哥哥</span></p><p class="utility-help">UI 与部分功能参考 CareUEyes。</p><dl class="compact-stats"><div><dt>今日专注</dt><dd><span data-stat="focus">0</span> 分钟</dd></div><div><dt>完成休息</dt><dd><span data-stat="rest">0</span> 次</dd></div><div><dt>休息时长</dt><dd><span data-stat="minutes">0</span> 分钟</dd></div></dl><p class="utility-help">${state.platform === "macos" && !state.keyboardProtection ? "快捷键防护需要在系统辅助功能中授权后重启。" : state.keyboardProtection ? "快捷键防护已就绪，休息期间启用。" : !this.native ? "浏览器仅预览界面，不拦截桌面快捷键。" : "快捷键防护状态由桌面权限决定。"}</p><div class="data-path"><span>本机记录</span><code>${esc(state.dataPath)}</code></div><button id="open-history" class="classic-button">休息记录</button>`,
      );
      this.button("open-history", () => this.navigate("history"));
    } else {
      this.logCount = this.read().logs.length;
      const labels: Record<string, string> = {
        completed: "完成休息",
        mindful: "正念解锁",
        emergency: "紧急解锁",
        rest: "开始休息",
        deferred: "会议保护",
        error: "需要留意",
      };
      panel.innerHTML = html(
        `<div class="history-heading"><h2>休息记录</h2><button id="history-back" class="classic-button">返回</button></div>${
          this.logCount
            ? `<div class="compact-log-list">${this.read()
                .logs.map(
                  (log) =>
                    `<article class="log-entry"><time>${new Date(log.at).toLocaleString(getLocale() === "en" ? "en-US" : "zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" })}</time><b>${labels[log.kind] || "记录"}</b><p>${esc(log.detail)}</p></article>`,
                )
                .join("")}</div>`
            : '<p class="empty-log">还没有休息记录。</p>'
        }`,
      );
      this.button("history-back", () => this.navigate("about"));
    }
  }
  update() {
    const state = this.read();
    const s = state.settings;
    for (const [id, value] of Object.entries({
      floating: s.floating,
      edgeHide: s.edgeHide,
      clickThrough: s.clickThrough,
      notifications: s.notifications,
      emergencyEnabled: s.emergencyEnabled,
      displayEnabled: s.displayEnabled,
    })) {
      const input = this.root.querySelector<HTMLInputElement>("#" + id);
      if (input) input.checked = value;
    }
    for (const [id, value, suffix] of [
      ["temperature", s.temperatureKelvin, "K"],
      ["brightness", s.brightnessPercent, "%"],
    ] as const) {
      const input = this.root.querySelector<HTMLInputElement>("#" + id);
      if (input && document.activeElement !== input) {
        input.value = String(value);
        this.root.querySelector("#" + id + "-value")!.textContent = tr(
          value + suffix,
        );
      }
    }
    const displayStatus = this.root.querySelector("#display-status");
    if (displayStatus)
      displayStatus.textContent = tr(
        !this.native
          ? "桌面版可应用到全部显示器"
          : state.displayStatus?.error ||
              (state.displayStatus?.supported === false
                ? "当前仅支持 Windows"
                : s.displayEnabled
                  ? "对全部显示器生效"
                  : "屏幕调节已关闭"),
      );
    for (const [id, key, output] of [
      ["work-minutes", "workMinutes", "work-value"],
      ["rest-minutes", "restMinutes", "rest-value"],
    ] as const) {
      const input = this.root.querySelector<HTMLInputElement>("#" + id);
      if (input && document.activeElement !== input) {
        input.value = String(s[key]);
        this.root.querySelector("#" + output)!.textContent = tr(
          s[key] + " 分钟",
        );
      }
    }
    this.root
      .querySelectorAll<HTMLButtonElement>("[data-rhythm]")
      .forEach((button) =>
        button.classList.toggle(
          "selected",
          button.dataset.rhythm === s.workMinutes + "," + s.restMinutes,
        ),
      );
    this.root
      .querySelectorAll<HTMLButtonElement>("[data-theme]")
      .forEach((button) => {
        button.classList.toggle("selected", button.dataset.theme === s.theme);
        if (button.dataset.theme === "custom")
          button.disabled = !s.customWallpaper;
        button.setAttribute(
          "aria-pressed",
          String(button.dataset.theme === s.theme),
        );
      });
    const sample = this.root.querySelector("#preset-preview");
    if (sample) sample.textContent = tr(s.phrase);
    const preset = this.root.querySelector<HTMLSelectElement>("#phrase-preset");
    if (preset) preset.value = String(state.phrases.indexOf(s.phrase));
    const desc = this.root.querySelector("#rhythm-description");
    if (desc)
      desc.textContent = tr(
        `专注 ${s.workMinutes} 分钟，休息 ${s.restMinutes} 分钟。关闭窗口后在托盘继续计时。`,
      );
    if (this.page === "history" && this.logCount !== state.logs.length)
      this.renderPanel();
  }
}
