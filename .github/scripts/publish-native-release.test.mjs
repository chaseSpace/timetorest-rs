import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import publish, { prepareNativeRelease } from "./publish-native-release.mjs";

const root = process.cwd();
const version = /^version = "([^"]+)"/m.exec(
  readFileSync("crates/leafshield-native/Cargo.toml", "utf8"),
)?.[1];
const prerelease = version.includes("-");
const tag = prerelease ? `win-v${version}` : `v${version}`;

test("prepares the three complete native release assets", () => {
  const release = prepareNativeRelease(root, tag);
  assert.equal(release.version, version);
  assert.deepEqual(
    release.assets.map((asset) => asset.name),
    [`leafshield-native-${version}.exe`, "LICENSE", "SHA256SUMS.txt"],
  );
  assert(release.assets.every((asset) => asset.data.length > 0));
  const wrongTag = prerelease ? `v${version}` : `win-v${version}`;
  assert.throws(() => prepareNativeRelease(root, wrongTag), /mismatch/);
});

test("publishes an immutable release after every asset uploads", async () => {
  const calls = [];
  const github = {
    rest: {
      repos: {
        getReleaseByTag: async () => {
          const error = new Error("missing");
          error.status = 404;
          throw error;
        },
        createRelease: async (request) => {
          calls.push(["create", request]);
          return { data: { id: 7, draft: true, assets: [] } };
        },
        uploadReleaseAsset: async (request) => {
          calls.push(["upload", request.name]);
          return {
            data: { state: "uploaded", size: request.data.length },
          };
        },
        updateRelease: async (request) => {
          calls.push(["publish", request]);
          return { data: { html_url: "https://example.invalid/release" } };
        },
      },
    },
  };
  const previousWorkspace = process.env.GITHUB_WORKSPACE;
  const previousTag = process.env.GITHUB_REF_NAME;
  process.env.GITHUB_WORKSPACE = root;
  process.env.GITHUB_REF_NAME = tag;
  try {
    await publish({
      github,
      context: { repo: { owner: "test", repo: "leafshield" }, sha: "abc" },
      core: {
        info() {},
        summary: { addLink() { return this; }, async write() {} },
      },
    });
  } finally {
    process.env.GITHUB_WORKSPACE = previousWorkspace;
    process.env.GITHUB_REF_NAME = previousTag;
  }
  assert.equal(calls[0][0], "create");
  assert.equal(calls[0][1].draft, true);
  assert.equal(calls[0][1].prerelease, prerelease);
  assert.deepEqual(calls.slice(1, 4).map((call) => call[1]), [
    `leafshield-native-${version}.exe`,
    "LICENSE",
    "SHA256SUMS.txt",
  ]);
  assert.equal(calls[4][0], "publish");
  assert.equal(calls[4][1].draft, false);
  assert.equal(calls[4][1].prerelease, prerelease);
  assert.equal(calls[4][1].make_latest, prerelease ? "false" : "true");
});
