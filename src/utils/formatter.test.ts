import { describe, expect, it } from "vitest";
import {
  fmtBps,
  fmtBytes,
  fmtBytesPerSec,
  fmtDate,
  fmtMs,
  nv,
  shortenIpList,
} from "./formatter";

describe("formatter utilities", () => {
  it("normalizes missing display values", () => {
    expect(nv(null)).toBe("-");
    expect(nv("  ")).toBe("-");
    expect(nv(0)).toBe("0");
  });

  it("formats throughput using decimal units", () => {
    expect(fmtBps(1_500_000)).toBe("1.50 Mbps");
    expect(fmtBytesPerSec(1_500_000)).toBe("1.50 MB/s");
  });

  it("formats byte totals using binary thresholds", () => {
    expect(fmtBytes(1_536)).toBe("1.50 KB");
    expect(fmtBytes(Number.NaN)).toBe("0 B");
  });

  it("formats optional milliseconds", () => {
    expect(fmtMs(0)).toBe("0 ms");
    expect(fmtMs(null)).toBe("-");
  });

  it("formats Rust SystemTime payloads", () => {
    const formatted = fmtDate({
      secs_since_epoch: 0,
      nanos_since_epoch: 500_000_000,
    });
    expect(formatted).not.toBe("-");
  });

  it("shortens interface address lists", () => {
    expect(shortenIpList([])).toBe("-");
    expect(
      shortenIpList([
        { addr: "192.0.2.1", prefix_len: 24 },
        { addr: "192.0.2.2", prefix_len: 24 },
      ]),
    ).toBe("192.0.2.1/24 + 1");
  });
});
