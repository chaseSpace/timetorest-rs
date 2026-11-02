import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import assert from "node:assert/strict";

export function prepareMacAssets(root, tag) {
  const version = JSON.parse(
    fs.readFileSync(path.join(root, "package.json"), "utf8"),
  ).version;
  assert(
    /^v\d+\.\d+\.\d+$/.test(tag) && tag === `v${version}`,
    "Tag/version mismatch",
  );
  const name = `LeafShield_${version}_universal.dmg`;
  const data = fs.readFileSync(path.join(root, "artifacts", name));
  assert(
    data.length > 512 && data.subarray(-512, -508).toString() === "koly",
    "Invalid DMG trailer",
  );
  const license = fs.readFileSync(path.join(root, "LICENSE"));
  const digest = (value) =>
    crypto.createHash("sha256").update(value).digest("hex");
  return [
    { name, data },
    {
      name: "SHA256SUMS-macos.txt",
      data: Buffer.from(
        `${digest(data)}  ${name}\n${digest(license)}  LICENSE\n`,
      ),
    },
  ];
}

export default async function publish({ github, context, core }) {
  const tag = process.env.SOURCE_TAG;
  const assets = prepareMacAssets(process.env.GITHUB_WORKSPACE, tag);
  const repo = context.repo;
  const version = tag.slice(1);
  const windowsAssets = [
    `leafshield-${version}.exe`,
    `LeafShield_${version}_x64-setup.exe`,
    "LICENSE",
    "SHA256SUMS.txt",
  ];
  let release;
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      const candidate = (
        await github.rest.repos.getReleaseByTag({ ...repo, tag })
      ).data;
      if (
        !candidate.draft &&
        windowsAssets.every((name) =>
          candidate.assets.some(
            (asset) => asset.name === name && asset.state === "uploaded",
          ),
        )
      ) {
        release = candidate;
        break;
      }
    } catch (error) {
      if (error.status !== 404) throw error;
    }
    core.info("Waiting for the Windows release to finish publishing");
    await new Promise((resolve) => setTimeout(resolve, 30000));
  }
  assert(release, "The matching Windows release was not published in time");
  if (/^[a-f0-9]{40}$/.test(release.target_commitish))
    assert.equal(
      release.target_commitish,
      process.env.SOURCE_SHA,
      "Release source commit differs",
    );
  // Hash the published license bytes: Windows checkout may use CRLF, whereas
  // the application source checked out on macOS uses LF.
  const licenseAsset = release.assets.find((asset) => asset.name === "LICENSE");
  let licenseHash = /^sha256:([a-f0-9]{64})$/.exec(
    licenseAsset.digest || "",
  )?.[1];
  if (!licenseHash) {
    const response = await fetch(licenseAsset.browser_download_url);
    assert(response.ok, "Could not read the published MIT license");
    licenseHash = crypto
      .createHash("sha256")
      .update(Buffer.from(await response.arrayBuffer()))
      .digest("hex");
  }
  const dmgHash = crypto
    .createHash("sha256")
    .update(assets[0].data)
    .digest("hex");
  assets[1].data = Buffer.from(
    `${dmgHash}  ${assets[0].name}\n${licenseHash}  LICENSE\n`,
  );
  const complete = assets.every(({ name }) =>
    release.assets.some(
      (item) => item.name === name && item.state === "uploaded",
    ),
  );
  if (!complete) {
    for (const asset of assets) {
      const existing = release.assets.find((item) => item.name === asset.name);
      if (existing)
        await github.rest.repos.deleteReleaseAsset({
          ...repo,
          asset_id: existing.id,
        });
      const uploaded = (
        await github.rest.repos.uploadReleaseAsset({
          ...repo,
          release_id: release.id,
          name: asset.name,
          data: asset.data,
          headers: {
            "content-type": "application/octet-stream",
            "content-length": asset.data.length,
          },
        })
      ).data;
      assert(
        uploaded.state === "uploaded" && uploaded.size === asset.data.length,
        `Incomplete asset: ${asset.name}`,
      );
    }
  }
  const marker = "<!-- macOS universal assets -->";
  if (!(release.body || "").includes(marker)) {
    const body = `${release.body || ""}\n\n---\n\n${marker}\n### macOS Universal\n\n- \`${assets[0].name}\`: Apple Silicon + Intel 通用安装镜像 / Universal installer.\n- \`SHA256SUMS-macos.txt\`: DMG 与 LICENSE 校验值 / DMG and license checksums.\n\n使用 ad-hoc 签名，未提供 Apple Developer ID 签名或公证。首次打开需按 macOS 提示授权。\nAd-hoc signed; not Developer ID signed or notarized. Follow macOS first-launch approval prompts.\n\nCI 已检查 arm64 / x86_64 架构及签名完整性；真机交互仍需手动验收。\nCI verifies both architectures and signature integrity; real-device interaction remains subject to manual acceptance.\n`;
    await github.rest.repos.updateRelease({
      ...repo,
      release_id: release.id,
      body,
    });
  }
  core.info(`macOS assets available at ${release.html_url}`);
  await core.summary
    .addLink("macOS Universal release", release.html_url)
    .write();
}
