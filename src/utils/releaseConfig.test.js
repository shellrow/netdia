import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const json = (path) => JSON.parse(readFileSync(path, "utf8"));

describe("application packaging configuration", () => {
  it("keeps application versions consistent", () => {
    const version = json("package.json").version;
    expect(json("src-tauri/tauri.conf.json").version).toBe(version);
    expect(json("package-lock.json").packages[""].version).toBe(version);
    const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
    expect(cargo.match(/^version = "([^"]+)"/m)?.[1]).toBe(version);
  });

  it("preserves the base security policy for Store builds", () => {
    const base = json("src-tauri/tauri.conf.json");
    const store = json("src-tauri/tauri.microsoftstore.conf.json");
    expect(base.app.security.csp["default-src"]).toBe("'self'");
    expect(store.app).toBeUndefined();
    expect(store.version).toBeUndefined();
    expect(store.bundle.createUpdaterArtifacts).toBe(false);
  });
});
