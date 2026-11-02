import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";

test.beforeEach(async ({ page }) => {
  const root = resolve("dist");
  await page.route("**/*", async (route) => {
    const url = new URL(route.request().url());
    if (url.hostname !== "leafshield.test") return route.abort();
    const path = resolve(
      root,
      "." +
        decodeURIComponent(url.pathname === "/" ? "/index.html" : url.pathname),
    );
    if (!path.startsWith(root + sep)) return route.abort();
    try {
      await route.fulfill({
        body: await readFile(path),
        contentType:
          (
            {
              ".html": "text/html; charset=utf-8",
              ".js": "text/javascript",
              ".css": "text/css",
              ".svg": "image/svg+xml",
              ".jpg": "image/jpeg",
              ".jpeg": "image/jpeg",
              ".png": "image/png",
              ".webp": "image/webp",
            } as Record<string, string>
          )[extname(path)] || "application/octet-stream",
      });
    } catch {
      await route.fulfill({ status: 404, body: "Not found" });
    }
  });
  await page.goto("https://leafshield.test/");
  await page.getByRole("button", { name: "计时器", exact: true }).click();
  await expect(page.getByRole("button", { name: "暂停计时" })).toBeVisible();
});
async function openHistory(page: import("@playwright/test").Page) {
  await page.getByRole("button", { name: "关于", exact: true }).click();
  await page.getByRole("button", { name: "休息记录", exact: true }).click();
}

test("compact timer pauses and every settings page fits 620x390", async ({
  page,
}) => {
  await page.getByRole("button", { name: "暂停计时" }).click();
  const before = await page.locator("[data-timer]").textContent();
  await page.waitForTimeout(1200);
  await expect(page.locator("[data-timer]")).toHaveText(before!);
  await page.getByRole("button", { name: "继续专注" }).click();
  await expect
    .poll(() => page.locator("[data-timer]").textContent())
    .not.toBe(before);
  for (const tab of [
    "显示器",
    "计时器",
    "悬浮窗",
    "解锁文案",
    "选项",
    "关于",
  ]) {
    await page.getByRole("button", { name: tab, exact: true }).click();
    expect(
      await page.evaluate(
        () =>
          document.documentElement.scrollWidth <= innerWidth &&
          document.documentElement.scrollHeight <= innerHeight,
      ),
    ).toBeTruthy();
    expect(
      await page
        .locator("#utility-panel")
        .evaluate((element) => element.scrollHeight <= element.clientHeight),
    ).toBeTruthy();
  }
  await page.getByRole("button", { name: "计时器", exact: true }).click();
  await page.screenshot({ path: "artifacts/ui-compact-home.png" });
});

test("only five read-only phrase presets are available and selection persists", async ({
  page,
}) => {
  await page.getByRole("button", { name: "解锁文案", exact: true }).click();
  await expect(page.locator("#phrase-preset option")).toHaveCount(5);
  await expect(
    page.locator("#utility-panel textarea,#utility-panel input[type=text]"),
  ).toHaveCount(0);
  await expect(page.getByText("自定义文案", { exact: true })).toHaveCount(0);
  await page.getByLabel("选择解锁文案", { exact: true }).selectOption("2");
  await expect(page.locator("#save-result")).toHaveText("设置已保存");
  const chosen = await page.locator("#preset-preview").textContent();
  await page.reload();
  await page.getByRole("button", { name: "解锁文案", exact: true }).click();
  await expect(page.locator("#phrase-preset")).toHaveValue("2");
  await expect(page.locator("#preset-preview")).toHaveText(chosen!);
  await page.screenshot({ path: "artifacts/ui-compact-phrases.png" });
});

test("legacy custom text resets to a preset while preserving duration and theme", async ({
  page,
}) => {
  await page.evaluate(() =>
    localStorage.setItem(
      "leafshield-preview",
      JSON.stringify({
        workMinutes: 35,
        theme: "night",
        phrase: "旧版自定义文案在新版中不应继续保留",
      }),
    ),
  );
  await page.reload();
  await page.getByRole("button", { name: "计时器", exact: true }).click();
  await expect(page.locator("#work-minutes")).toHaveValue("35");
  await page.getByRole("button", { name: "解锁文案", exact: true }).click();
  await expect(page.locator("#phrase-preset")).toHaveValue("0");
  await expect(page.locator("#preset-preview")).not.toContainText("旧版自定义");
  await page.getByRole("button", { name: "选项", exact: true }).click();
  await expect(page.locator("[data-theme=night]")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
});

test("rhythm and docking options persist without a large settings form", async ({
  page,
}) => {
  await page.locator('[data-rhythm="25,2"]').click();
  await expect(page.locator("#save-result")).toHaveText("设置已保存");
  await page.getByRole("button", { name: "悬浮窗", exact: true }).click();
  await expect(page.locator("#edgeHide")).toBeChecked();
  await page.locator("#edgeHide").uncheck();
  await expect(page.locator("#save-result")).toHaveText("设置已保存");
  await page.locator("#clickThrough").check();
  await expect(page.locator("#save-result")).toHaveText("设置已保存");
  await page.reload();
  await page.getByRole("button", { name: "计时器", exact: true }).click();
  await expect(page.locator("#work-minutes")).toHaveValue("25");
  await expect(page.locator("#rest-minutes")).toHaveValue("2");
  await page.getByRole("button", { name: "悬浮窗", exact: true }).click();
  await expect(page.locator("#edgeHide")).not.toBeChecked();
  await expect(page.locator("#clickThrough")).toBeChecked();
  await page.screenshot({ path: "artifacts/ui-compact-widget-settings.png" });
});

test("startup switch roundtrips independently of timer settings", async ({
  page,
}) => {
  await page.getByRole("button", { name: "选项", exact: true }).click();
  const startup = page.locator("#startup-enabled");
  await expect(startup).toBeEnabled();
  await expect(startup).toBeChecked();
  await startup.uncheck();
  await expect(page.locator("#save-result")).toHaveText("已关闭开机启动");
  await page.reload();
  await page.getByRole("button", { name: "选项", exact: true }).click();
  await expect(startup).not.toBeChecked();
  await startup.check();
  await expect(page.locator("#save-result")).toHaveText("已设置开机启动");
  await page.reload();
  await page.getByRole("button", { name: "计时器", exact: true }).click();
  await expect(page.locator("#work-minutes")).toHaveValue("45");
  await expect(page.locator("#rest-minutes")).toHaveValue("2");
  await page.getByRole("button", { name: "选项", exact: true }).click();
  await expect(startup).toBeChecked();
  await page.screenshot({ path: "artifacts/ui-startup-options.png" });
});

test("widget collapses to a small edge tab and restores the timer content", async ({
  page,
}) => {
  await page.setViewportSize({ width: 176, height: 54 });
  await page.goto("https://leafshield.test/index.html#widget");
  await expect(page.getByRole("button", { name: "打开一叶" })).toBeVisible();
  await page.evaluate(() =>
    window.dispatchEvent(
      new CustomEvent("preview-widget-dock", {
        detail: { edge: "right", hidden: true },
      }),
    ),
  );
  await page.setViewportSize({ width: 8, height: 40 });
  await expect(page.locator(".dock-tab")).toBeVisible();
  await expect(page.locator(".widget-expanded")).toBeHidden();
  expect(
    await page.evaluate(
      () =>
        document.documentElement.scrollWidth <= innerWidth &&
        document.documentElement.scrollHeight <= innerHeight,
    ),
  ).toBeTruthy();
  await page.evaluate(() =>
    window.dispatchEvent(
      new CustomEvent("preview-widget-dock", {
        detail: { edge: "right", hidden: false },
      }),
    ),
  );
  await page.setViewportSize({ width: 176, height: 54 });
  await expect(page.getByRole("button", { name: "打开一叶" })).toBeVisible();
  await page.screenshot({ path: "artifacts/ui-compact-widget.png" });
});

test("rest still rejects paste and unlocks only after exact manual input", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1180, height: 800 });
  await page.getByRole("button", { name: "现在休息", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "正念解锁", exact: true }),
  ).toBeDisabled();
  const blocked = await page
    .locator("#unlock-input")
    .evaluate((input: HTMLInputElement) => {
      const data = new DataTransfer();
      data.setData("text/plain", "不允许粘贴");
      const event = new ClipboardEvent("paste", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      });
      input.dispatchEvent(event);
      return event.defaultPrevented;
    });
  expect(blocked).toBeTruthy();
  await expect(page.locator("#unlock-input")).toHaveValue("");
  await page.locator("#unlock-input").fill("我想跳过");
  await expect(
    page.getByRole("button", { name: "正念解锁", exact: true }),
  ).toBeDisabled();
  await page.locator("#unlock-input").clear();
  const phrase = await page.locator("#unlock-phrase").textContent();
  await page.locator("#unlock-input").pressSequentially(phrase!, { delay: 5 });
  await page.getByRole("button", { name: "正念解锁", exact: true }).click();
  await openHistory(page);
  await expect(page.locator(".log-entry")).toContainText("正念解锁");
});

test("emergency still needs fifty physical presses and logs the unlock", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1180, height: 800 });
  await page.getByRole("button", { name: "现在休息", exact: true }).click();
  await page.getByRole("button", { name: "突发紧急情况？" }).click();
  await page.keyboard.down("a");
  await page.waitForTimeout(70);
  await page.keyboard.down("a");
  await expect(page.locator("#emergency-count")).toHaveText("1");
  await page.keyboard.up("a");
  for (let i = 0; i < 49; i++) {
    await page.waitForTimeout(45);
    await page.keyboard.press("a");
  }
  await page.getByRole("button", { name: "计时器", exact: true }).click();
  await expect(page.getByRole("button", { name: "暂停计时" })).toBeVisible();
  await openHistory(page);
  await expect(page.locator(".log-entry")).toContainText("紧急解锁");
});

test("preview returns to compact main window without creating a log", async ({
  page,
}) => {
  await page.getByRole("button", { name: "解锁文案", exact: true }).click();
  await page.getByRole("button", { name: "预览休息界面", exact: true }).click();
  await page.getByRole("button", { name: "关闭预览" }).click();
  await expect(page.locator(".utility-titlebar")).toBeVisible();
  await openHistory(page);
  await expect(page.getByText("还没有休息记录。")).toBeVisible();
});

test("all pages hide scrollbars and long logs still scroll", async ({
  page,
}) => {
  for (const tab of [
    "显示器",
    "计时器",
    "悬浮窗",
    "解锁文案",
    "选项",
    "关于",
  ]) {
    await page.getByRole("button", { name: tab, exact: true }).click();
    const styles = await page.evaluate(() =>
      [
        document.documentElement,
        document.body,
        document.querySelector("#utility-panel")!,
        ...document.querySelectorAll("textarea"),
      ].map((element) => ({
        width: getComputedStyle(element).scrollbarWidth,
        display: getComputedStyle(element, "::-webkit-scrollbar").display,
      })),
    );
    expect(
      styles.every(
        (style) => style.width === "none" && style.display === "none",
      ),
    ).toBeTruthy();
  }
  await page.getByRole("button", { name: "计时器", exact: true }).click();
  for (let i = 0; i < 6; i++) {
    await page.getByRole("button", { name: "现在休息", exact: true }).click();
    const phrase = await page.locator("#unlock-phrase").textContent();
    await page.locator("#unlock-input").fill(phrase!);
    await page.getByRole("button", { name: "正念解锁", exact: true }).click();
    await page.getByRole("button", { name: "计时器", exact: true }).click();
  }
  await openHistory(page);
  const log = page.locator(".compact-log-list");
  expect(
    await log.evaluate(
      (element) => element.scrollHeight > element.clientHeight,
    ),
  ).toBeTruthy();
  await log.hover();
  await page.mouse.wheel(0, 500);
  await expect
    .poll(() => log.evaluate((element) => element.scrollTop))
    .toBeGreaterThan(0);
  expect(
    await log.evaluate(
      (element) =>
        getComputedStyle(element).scrollbarWidth === "none" &&
        getComputedStyle(element, "::-webkit-scrollbar").display === "none",
    ),
  ).toBeTruthy();
  await page.screenshot({ path: "artifacts/ui-compact-logs.png" });
});

test("new lock screen uses three bundled photos and removes the old painted page", async ({
  page,
}) => {
  for (const theme of ["forest", "mountain", "night"]) {
    await page.getByRole("button", { name: "选项", exact: true }).click();
    await page.locator(`[data-theme=${theme}]`).click();
    await expect(page.locator("#save-result")).toHaveText("设置已保存");
    await page.getByRole("button", { name: "预览", exact: true }).click();
    await expect(page.locator(".lock-photo")).toHaveAttribute(
      "src",
      `/wallpapers/${theme}.jpg`,
    );
    await expect
      .poll(() =>
        page
          .locator(".lock-photo")
          .evaluate((img: HTMLImageElement) => img.naturalWidth),
      )
      .toBe(1600);
    await expect(
      page.locator(".rest-screen,.rest-hill,.breathing-guide"),
    ).toHaveCount(0);
    if (theme === "forest") {
      await page.setViewportSize({ width: 1180, height: 800 });
      await page.screenshot({ path: "artifacts/ui-photo-lock.png" });
      await page.setViewportSize({ width: 620, height: 390 });
    }
    expect(
      await page.evaluate(
        () => document.documentElement.scrollHeight <= innerHeight,
      ),
    ).toBeTruthy();
    await page.getByRole("button", { name: "关闭预览" }).click();
  }
});

test("custom background is compressed, persists, and can be reselected", async ({
  page,
}) => {
  await page.getByRole("button", { name: "选项", exact: true }).click();
  await expect(page.locator("[data-theme=custom]")).toBeDisabled();
  await page
    .locator("#wallpaper-file")
    .setInputFiles("public/wallpapers/night.jpg");
  await expect(page.locator("#save-result")).toHaveText("自定义背景已保存");
  await expect(page.locator("[data-theme=custom]")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  const stored = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("leafshield-wallpaper")!),
  );
  expect(stored.name).toMatch(/^custom-[a-f0-9]{64}\.jpg$/);
  expect(
    Buffer.from(stored.dataUrl.split(",")[1], "base64").length,
  ).toBeLessThanOrEqual(500_000);
  await page.screenshot({ path: "artifacts/ui-photo-options.png" });
  await page.locator("[data-theme=forest]").click();
  await expect(page.locator("#save-result")).toHaveText("设置已保存");
  await page.reload();
  await page.getByRole("button", { name: "选项", exact: true }).click();
  await page.locator("[data-theme=custom]").click();
  await expect(page.locator("#save-result")).toHaveText("设置已保存");
  await page.getByRole("button", { name: "预览", exact: true }).click();
  await expect(page.locator(".lock-photo")).toHaveAttribute(
    "src",
    stored.dataUrl,
  );
  await expect
    .poll(() =>
      page
        .locator(".lock-photo")
        .evaluate((img: HTMLImageElement) => img.naturalWidth),
    )
    .toBeGreaterThan(0);
  await expect(page.locator("#photo-credit")).toHaveText("自定义背景");
});

test("oversize or corrupt uploads preserve the previous background", async ({
  page,
}) => {
  await page.getByRole("button", { name: "选项", exact: true }).click();
  await page.locator("#wallpaper-file").setInputFiles({
    name: "too-large.jpg",
    mimeType: "image/jpeg",
    buffer: Buffer.alloc(5_000_001),
  });
  await expect(page.locator("#save-result")).toHaveText(
    "请选择不超过 5 MB 的图片",
  );
  await page.locator("#wallpaper-file").setInputFiles({
    name: "corrupt.jpg",
    mimeType: "image/jpeg",
    buffer: Buffer.from("not a photo"),
  });
  await expect(page.locator("#toast")).toContainText(
    "仅支持 JPEG、PNG 或 WebP 图片",
  );
  await expect(page.locator("[data-theme=custom]")).toBeDisabled();
  await expect(page.locator("[data-theme=forest]")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
});

test("missing custom file falls back to bundled photo with working unlock controls", async ({
  page,
}) => {
  await page.getByRole("button", { name: "选项", exact: true }).click();
  await page
    .locator("#wallpaper-file")
    .setInputFiles("src-tauri/icons/128x128.png");
  await expect(page.locator("#save-result")).toHaveText("自定义背景已保存");
  await page.evaluate(() => localStorage.removeItem("leafshield-wallpaper"));
  await page.getByRole("button", { name: "预览", exact: true }).click();
  await expect(page.locator(".lock-photo")).toHaveAttribute(
    "src",
    "/wallpapers/forest.jpg",
  );
  await expect(page.locator("#toast")).toContainText("自定义图片已丢失");
  await expect(page.locator("#unlock-input")).toBeVisible();
});

test("display sliders persist and restoring the desktop removes both filters", async ({
  page,
}) => {
  await page.getByRole("button", { name: "显示器", exact: true }).click();
  await expect(page.locator("#temperature-value")).toHaveText("5000K");
  await expect(page.locator("#brightness-value")).toHaveText("80%");
  await page.screenshot({ path: "artifacts/ui-display.png" });
  await page.getByRole("slider", { name: "屏幕色温" }).focus();
  await page.keyboard.press("Home");
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("#save-result")).toHaveText("设置已保存");
  await page.getByRole("slider", { name: "屏幕亮度" }).focus();
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  await expect(page.locator("#save-result")).toHaveText("设置已保存");
  await page.reload();
  await expect(page.locator("#temperature")).toHaveValue("1600");
  await expect(page.locator("#brightness")).toHaveValue("99");
  await page.getByRole("button", { name: "恢复原色", exact: true }).click();
  await expect(page.locator("#temperature")).toHaveValue("6500");
  await expect(page.locator("#brightness")).toHaveValue("100");
  await page.locator("#displayEnabled").uncheck();
  await expect(page.locator("#save-result")).toHaveText("设置已保存");
  await page.reload();
  await expect(page.locator("#displayEnabled")).not.toBeChecked();
});

test("program selection merges the draft, deduplicates and enforces five entries", async ({
  page,
}) => {
  await page.getByRole("button", { name: "选项", exact: true }).click();
  await page.locator("#whitelist").fill("Teams.exe");
  await page.locator("#whitelist-files").setInputFiles([
    {
      name: "Zoom.exe",
      mimeType: "application/octet-stream",
      buffer: Buffer.from("fixture"),
    },
    {
      name: "teams.EXE",
      mimeType: "application/octet-stream",
      buffer: Buffer.from("fixture"),
    },
  ]);
  await expect(page.locator("#save-result")).toHaveText("白名单已保存");
  await expect(page.locator("#whitelist")).toHaveValue("Teams.exe\nZoom.exe");
  await page.reload();
  await page.getByRole("button", { name: "选项", exact: true }).click();
  await expect(page.locator("#whitelist")).toHaveValue("Teams.exe\nZoom.exe");
  await page.screenshot({ path: "artifacts/ui-whitelist-picker.png" });
  const five = "one.exe\ntwo.exe\nthree.exe\nfour.exe\nfive.exe";
  await page.locator("#whitelist").fill(five);
  await page.locator("#whitelist-files").setInputFiles({
    name: "six.exe",
    mimeType: "application/octet-stream",
    buffer: Buffer.from("fixture"),
  });
  await expect(page.locator("#save-result")).toContainText("最多 5 个");
  await expect(page.locator("#whitelist")).toHaveValue(five);
  await page.locator("#whitelist-files").setInputFiles({
    name: "desktop.lnk",
    mimeType: "application/octet-stream",
    buffer: Buffer.from("fixture"),
  });
  await expect(page.locator("#save-result")).toContainText(
    "快捷方式请在桌面版选择",
  );
  await expect(page.locator("#whitelist")).toHaveValue(five);
});

test("drag preview avoids saving intermediate display values", async ({
  page,
}) => {
  await page.getByRole("button", { name: "显示器", exact: true }).click();
  const before = await page.evaluate(() =>
    localStorage.getItem("leafshield-preview"),
  );
  await page.locator("#temperature").evaluate((input: HTMLInputElement) => {
    input.focus();
    input.value = "3500";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await page.waitForTimeout(150);
  expect(
    await page.evaluate(() => localStorage.getItem("leafshield-preview")),
  ).toBe(before);
  await expect(page.locator("#temperature-value")).toHaveText("3500K");
  await page.locator("#temperature").dispatchEvent("change");
  await expect(page.locator("#save-result")).toHaveText("设置已保存");
  expect(
    await page.evaluate(
      () =>
        JSON.parse(localStorage.getItem("leafshield-preview")!)
          .temperatureKelvin,
    ),
  ).toBe(3500);
  await page.getByRole("button", { name: "恢复原色", exact: true }).click();
  await expect(page.locator("#temperature")).toHaveValue("6500");
  await page.waitForTimeout(100);
  expect(
    await page.evaluate(
      () =>
        JSON.parse(localStorage.getItem("leafshield-preview")!)
          .temperatureKelvin,
    ),
  ).toBe(6500);
});
