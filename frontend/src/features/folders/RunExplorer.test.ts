import { describe, expect, it } from "vitest";

import { formatBytes, formatDuration } from "./runFormatting";

describe("run explorer formatting", () => {
  it("formats bounded byte values without losing the unit", () => {
    expect(formatBytes("0")).toBe("0 B");
    expect(formatBytes("1024")).toBe("1.00 KiB");
    expect(formatBytes("1847265280")).toBe("1.72 GiB");
  });

  it("formats sub-second and minute durations", () => {
    expect(formatDuration("850")).toBe("850 ms");
    expect(formatDuration("102000")).toBe("1m 42s");
  });
});
