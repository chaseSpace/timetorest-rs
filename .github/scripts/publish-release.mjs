import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createHash } from "node:crypto";

export function prepareRelease(root, tag) {
  const version = JSON.parse(
    readFileSync(join(root, "package.json"), "utf8"),
  ).version;
  if (!/^v\d+\.\d+\.\d+$/.test(tag) || tag !== `v${version}`)
    throw new Error("Tag/version mismatch");
  const body = readFileSync(join(root, "docs/releases", `${tag}.md`), "utf8");
  const names = [
    `leafshield-${version}.exe`,
    `LeafShield_${version}_x64-setup.exe`,
  ];
  const assets = names.map((name) => {
    const data = readFileSync(join(root, "artifacts", name));
    if (data.length < 1024 || data.subarray(0, 2).toString() !== "MZ")
      throw new Error(`Invalid Windows executable: ${name}`);
    return { name, data };
  });
  const checksums =
    assets
      .map(
        ({ name, data }) =>
          `${createHash("sha256").update(data).digest("hex")}  ${name}`,
      )
      .join("\n") + "\n";
  writeFileSync(join(root, "artifacts/SHA256SUMS.txt"), checksums);
  assets.push({ name: "SHA256SUMS.txt", data: Buffer.from(checksums) });
  return { version, tag, body, assets };
}

export default async function publish({ github, context, core }) {
  const release = prepareRelease(
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
    throw new Error(
      `Release ${release.tag} is already published; refusing to replace it`,
    );
  if (!draft) {
    draft = (
      await github.rest.repos.createRelease({
        ...repo,
        tag_name: release.tag,
        target_commitish: context.sha,
        name: `LeafShield ${release.tag}`,
        body: release.body,
        draft: true,
        prerelease: false,
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
        name: asset.name,
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
      prerelease: false,
      make_latest: "true",
    })
  ).data;
  core.info(`Published ${published.html_url}`);
  await core.summary
    .addLink(`LeafShield ${release.tag}`, published.html_url)
    .write();
}
