import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { basename, join } from "node:path";

const sha256 = (data) => createHash("sha256").update(data).digest("hex");

export function prepareNativeRelease(root, tag) {
  const manifest = readFileSync(
    join(root, "crates/leafshield-native/Cargo.toml"),
    "utf8",
  );
  const version = /^version = "([^"]+)"/m.exec(manifest)?.[1];
  const prerelease = version?.includes("-") ?? false;
  const expectedTag = prerelease ? `win-v${version}` : `v${version}`;
  if (!version || tag !== expectedTag)
    throw new Error("Tag/version mismatch");
  if (!/^\d+\.\d+\.\d+(?:-dev\.\d+)?$/.test(version))
    throw new Error("Native version must use x.y.z or x.y.z-dev.n");

  const directory = join(root, "artifacts/native-dev");
  const executableName = `leafshield-native-${version}.exe`;
  const names = [executableName, "LICENSE"];
  const assets = names.map((name) => ({
    name,
    data: readFileSync(join(directory, name)),
  }));
  const executable = assets[0].data;
  if (executable.length < 1024 || executable.subarray(0, 2).toString() !== "MZ")
    throw new Error("Invalid Windows executable");
  if (!assets[1].data.toString().startsWith("MIT License"))
    throw new Error("Invalid MIT license asset");

  const expectedChecksums =
    assets.map(({ name, data }) => `${sha256(data)}  ${name}`).join("\n") + "\n";
  const checksumData = readFileSync(join(directory, "SHA256SUMS.txt"));
  if (checksumData.toString().replace(/\r\n/g, "\n") !== expectedChecksums)
    throw new Error("SHA256SUMS.txt does not match release assets");
  assets.push({ name: "SHA256SUMS.txt", data: checksumData });

  const body = readFileSync(
    join(root, "docs/releases", `${tag}.md`),
    "utf8",
  );
  if (!body.includes("Rust + Win32") || !body.includes("macOS"))
    throw new Error("Native release notes must describe the stack and platform scope");
  return { version, tag, body, assets, prerelease };
}

export default async function publish({ github, context, core }) {
  const release = prepareNativeRelease(
    process.env.GITHUB_WORKSPACE,
    process.env.GITHUB_REF_NAME,
  );
  const repo = context.repo;
  let draft;
  try {
    draft = (
      await github.rest.repos.getReleaseByTag({ ...repo, tag: release.tag })
    ).data;
  } catch (error) {
    if (error.status !== 404) throw error;
  }
  if (draft && !draft.draft)
    throw new Error(`Release ${release.tag} is already published; refusing to replace it`);
  if (!draft) {
    draft = (
      await github.rest.repos.createRelease({
        ...repo,
        tag_name: release.tag,
        target_commitish: context.sha,
        name: `LeafShield Windows ${release.version}`,
        body: release.body,
        draft: true,
        prerelease: release.prerelease,
      })
    ).data;
  }
  for (const asset of release.assets) {
    const existing = draft.assets?.find((item) => item.name === asset.name);
    if (existing)
      await github.rest.repos.deleteReleaseAsset({
        ...repo,
        asset_id: existing.id,
      });
    const uploaded = (
      await github.rest.repos.uploadReleaseAsset({
        ...repo,
        release_id: draft.id,
        name: basename(asset.name),
        data: asset.data,
        headers: {
          "content-type": "application/octet-stream",
          "content-length": asset.data.length,
        },
      })
    ).data;
    if (uploaded.state !== "uploaded" || uploaded.size !== asset.data.length)
      throw new Error(`Incomplete release asset: ${asset.name}`);
  }
  const published = (
    await github.rest.repos.updateRelease({
      ...repo,
      release_id: draft.id,
      body: release.body,
      draft: false,
      prerelease: release.prerelease,
      make_latest: release.prerelease ? "false" : "true",
    })
  ).data;
  core.info(`Published ${published.html_url}`);
  await core.summary
    .addLink(`LeafShield Windows ${release.version}`, published.html_url)
    .write();
}
