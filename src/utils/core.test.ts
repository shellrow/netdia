import { describe, expect, it } from "vitest";
import { hexToRgba } from "./color";
import { clampInt } from "./numeric";
import { normalizeBpsUnit, readBpsUnit } from "./preferences";

describe("core utilities", () => {
  it("clamps values to an inclusive range", () => {
    expect(clampInt(-1, 0, 10)).toBe(0);
    expect(clampInt(12, 0, 10)).toBe(10);
    expect(clampInt(4, 0, 10)).toBe(4);
  });

  it("converts short and long hex colors", () => {
    expect(hexToRgba("#fff", 0.5)).toBe("rgba(255, 255, 255, 0.5)");
    expect(hexToRgba("336699", 1)).toBe("rgba(51, 102, 153, 1)");
  });

  it("clamps alpha and safely handles invalid colors", () => {
    expect(hexToRgba("invalid", 2)).toBe("rgba(0, 0, 0, 1)");
  });

  it("normalizes throughput preferences", () => {
    expect(normalizeBpsUnit("bytes")).toBe("bytes");
    expect(normalizeBpsUnit("unknown", "bits")).toBe("bits");
  });

  it("reads throughput preferences from storage", () => {
    const storage = {
      getItem: () => "bytes",
    } as Pick<Storage, "getItem"> as Storage;
    expect(readBpsUnit(storage)).toBe("bytes");
  });
});
