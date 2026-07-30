import { describe, expect, it } from "vitest";

import type { BrowserNode } from "../../shared/contracts/generated";
import {
  adjacentSelection,
  appendBounded,
  edgeSelection,
  flattenTreeRows,
  formatIec,
  selectableDirectoryPaths,
} from "./folderBrowserModel";

function node(
  path: string,
  kind: BrowserNode["kind"],
  available = true,
): BrowserNode {
  return {
    id: path,
    path,
    name: path,
    kind,
    available,
    is_mount_point: false,
    is_network_mount: false,
    is_platform_special: false,
    modified_at_unix_ms: null,
  };
}

describe("folder browser navigation", () => {
  it("skips files and unavailable directories", () => {
    expect(
      selectableDirectoryPaths([
        node("/a", "directory"),
        node("/file", "regular_file"),
        node("/offline", "directory", false),
        node("/b", "directory"),
      ]),
    ).toEqual(["/a", "/b"]);
  });

  it("bounds adjacent movement and resolves Home/End edges", () => {
    const paths = ["/a", "/b", "/c"];
    expect(adjacentSelection(paths, "/a", -1)).toBe("/a");
    expect(adjacentSelection(paths, "/b", 1)).toBe("/c");
    expect(adjacentSelection(paths, "/c", 1)).toBe("/c");
    expect(edgeSelection(paths, "first")).toBe("/a");
    expect(edgeSelection(paths, "last")).toBe("/c");
  });

  it("keeps a bounded window for large paged directories", () => {
    const first = Array.from({ length: 400 }, (_, index) => index);
    const next = Array.from({ length: 400 }, (_, index) => index + 400);
    const result = appendBounded(first, next, 500);

    expect(result).toHaveLength(500);
    expect(result[0]).toBe(300);
    expect(result.at(-1)).toBe(799);
  });

  it("flattens a deeply expanded tree without recursive stack growth", () => {
    const depth = 10_000;
    const nodes = Array.from({ length: depth }, (_, index) =>
      node(`/root/${index}`, "directory"),
    );
    const pages = new Map<string, { nodes: BrowserNode[] }>();
    pages.set("/root", { nodes: [nodes[0]!] });
    for (let index = 0; index < nodes.length - 1; index += 1) {
      pages.set(nodes[index]!.path, { nodes: [nodes[index + 1]!] });
    }

    const rows = flattenTreeRows(
      node("/root", "directory"),
      new Set(["/root", ...nodes.map((entry) => entry.path)]),
      pages,
    );

    expect(rows).toHaveLength(depth + 1);
    expect(rows.at(-1)).toEqual({ node: nodes.at(-1), depth });
  });
});

describe("folder size formatting", () => {
  it("uses compact IEC units", () => {
    expect(formatIec(0n)).toBe("0 B");
    expect(formatIec(1024n)).toBe("1.0 KiB");
    expect(formatIec(10n * 1024n * 1024n)).toBe("10 MiB");
  });
});
