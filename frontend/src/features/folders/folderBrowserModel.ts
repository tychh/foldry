import type { BrowserNode } from "../../shared/contracts/generated";

export function selectableDirectoryPaths(nodes: BrowserNode[]): string[] {
  return nodes
    .filter((node) => node.kind === "directory" && node.available)
    .map((node) => node.path);
}

export function adjacentSelection(
  paths: string[],
  current: string,
  direction: -1 | 1,
): string | null {
  if (!paths.length) return null;
  const index = Math.max(paths.indexOf(current), 0);
  return paths[Math.max(0, Math.min(paths.length - 1, index + direction))]!;
}

export function edgeSelection(
  paths: string[],
  edge: "first" | "last",
): string | null {
  return edge === "first" ? (paths[0] ?? null) : (paths.at(-1) ?? null);
}

export function appendBounded<T>(
  current: T[],
  next: T[],
  maximum: number,
): T[] {
  return [...current, ...next].slice(-Math.max(0, maximum));
}

export function flattenTreeRows(
  root: BrowserNode,
  expanded: ReadonlySet<string>,
  pages: ReadonlyMap<string, { nodes: BrowserNode[] }>,
): Array<{ node: BrowserNode; depth: number }> {
  const rows: Array<{ node: BrowserNode; depth: number }> = [];
  const pending = [{ node: root, depth: 0 }];

  while (pending.length > 0) {
    const current = pending.pop()!;
    rows.push(current);
    if (!expanded.has(current.node.path)) continue;

    const children = pages.get(current.node.path)?.nodes ?? [];
    for (let index = children.length - 1; index >= 0; index -= 1) {
      pending.push({ node: children[index]!, depth: current.depth + 1 });
    }
  }

  return rows;
}

export function formatIec(bytes: bigint): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = Number(bytes);
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 10 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}
