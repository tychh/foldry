import {
  ActionIcon,
  Badge,
  Box,
  Loader,
  Stack,
  Text,
  Tooltip,
} from "@mantine/core";
import {
  CaretDown,
  CaretRight,
  Folder,
  HardDrive,
  Link,
  Plus,
  WarningCircle,
  WifiHigh,
} from "@phosphor-icons/react";
import { useState } from "react";

import type {
  BootstrapSnapshot,
  BrowserChildren,
  BrowserNode,
  BrowserRoot,
} from "../../shared/contracts/generated";
import { useI18n } from "../../shared/i18n/I18nProvider";
import { useDesktopData } from "../../shared/ipc/DesktopDataProvider";
import classes from "./TasksWorkspace.module.css";

type FolderTreeProps = {
  roots: BootstrapSnapshot["roots"];
  onAdd: (path: string) => void;
};

type ChildState =
  | { status: "loading"; nodes: BrowserNode[] }
  | { status: "loaded"; nodes: BrowserNode[] }
  | { status: "error"; nodes: BrowserNode[] };

export function FolderTree({ roots, onAdd }: FolderTreeProps) {
  const { query } = useDesktopData();
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [children, setChildren] = useState<ReadonlyMap<string, ChildState>>(
    () => new Map(),
  );

  const toggle = async (path: string) => {
    if (expanded.has(path)) {
      setExpanded((current) => {
        const next = new Set(current);
        next.delete(path);
        return next;
      });
      void query<boolean>("cancel_browser_request", { path }).catch(
        () => undefined,
      );
      return;
    }
    setExpanded((current) => new Set(current).add(path));
    if (children.get(path)?.status === "loaded") {
      return;
    }
    setChildren((current) => {
      const next = new Map(current);
      next.set(path, { status: "loading", nodes: [] });
      return next;
    });
    try {
      const response = await query<BrowserChildren>("browser_children", {
        path,
      });
      setChildren((current) => {
        const next = new Map(current);
        next.set(path, { status: "loaded", nodes: response.nodes });
        return next;
      });
    } catch {
      setChildren((current) => {
        const next = new Map(current);
        next.set(path, { status: "error", nodes: [] });
        return next;
      });
    }
  };

  return (
    <Stack gap={2}>
      {roots.map((root) => (
        <TreeBranch
          childState={children.get(root.path)}
          depth={0}
          expanded={expanded}
          item={rootItem(root)}
          key={root.id}
          onAdd={onAdd}
          onToggle={(path) => void toggle(path)}
          renderChildren={(node, depth) => (
            <TreeBranch
              childState={children.get(node.path)}
              depth={depth}
              expanded={expanded}
              item={node}
              key={node.id}
              onAdd={onAdd}
              onToggle={(path) => void toggle(path)}
              renderChildren={(child, childDepth) => (
                <RecursiveBranch
                  childState={children}
                  depth={childDepth}
                  expanded={expanded}
                  item={child}
                  key={child.id}
                  onAdd={onAdd}
                  onToggle={(path) => void toggle(path)}
                />
              )}
            />
          )}
        />
      ))}
    </Stack>
  );
}

function RecursiveBranch({
  item,
  depth,
  expanded,
  childState,
  onToggle,
  onAdd,
}: {
  item: BrowserNode;
  depth: number;
  expanded: ReadonlySet<string>;
  childState: ReadonlyMap<string, ChildState>;
  onToggle: (path: string) => void;
  onAdd: (path: string) => void;
}) {
  return (
    <TreeBranch
      childState={childState.get(item.path)}
      depth={depth}
      expanded={expanded}
      item={item}
      onAdd={onAdd}
      onToggle={onToggle}
      renderChildren={(child, childDepth) => (
        <RecursiveBranch
          childState={childState}
          depth={childDepth}
          expanded={expanded}
          item={child}
          key={child.id}
          onAdd={onAdd}
          onToggle={onToggle}
        />
      )}
    />
  );
}

function TreeBranch({
  item,
  depth,
  expanded,
  childState,
  onToggle,
  onAdd,
  renderChildren,
}: {
  item: BrowserNode;
  depth: number;
  expanded: ReadonlySet<string>;
  childState: ChildState | undefined;
  onToggle: (path: string) => void;
  onAdd: (path: string) => void;
  renderChildren: (node: BrowserNode, depth: number) => React.ReactNode;
}) {
  const { t } = useI18n();
  const open = expanded.has(item.path);
  const directory = item.kind === "directory";
  return (
    <Box>
      <Box
        className={classes.treeRow}
        data-unavailable={!item.available || undefined}
        style={{ paddingInlineStart: `${depth * 14 + 4}px` }}
      >
        <ActionIcon
          aria-label={open ? t("collapseFolder") : t("expandFolder")}
          disabled={!directory || !item.available}
          size="sm"
          variant="subtle"
          onClick={() => onToggle(item.path)}
        >
          {childState?.status === "loading" ? (
            <Loader size={12} />
          ) : open ? (
            <CaretDown aria-hidden size={13} />
          ) : (
            <CaretRight aria-hidden size={13} />
          )}
        </ActionIcon>
        <button
          className={classes.treeSelect}
          disabled={!directory || !item.available}
          title={item.path}
          type="button"
          onClick={() => onAdd(item.path)}
        >
          <TreeIcon item={item} />
          <Text className={classes.treeLabel} component="span" size="sm">
            {item.name}
          </Text>
        </button>
        <TreeIndicators item={item} />
        <Tooltip label={t("addFolderTask")}>
          <ActionIcon
            aria-label={`${t("addFolderTask")}: ${item.name}`}
            disabled={!directory || !item.available}
            className={classes.treeAdd}
            size="sm"
            variant="subtle"
            onClick={() => onAdd(item.path)}
          >
            <Plus aria-hidden size={13} />
          </ActionIcon>
        </Tooltip>
      </Box>
      {open && childState?.status === "error" ? (
        <Text
          c="red"
          className={classes.treeMessage}
          size="xs"
          style={{ paddingInlineStart: `${(depth + 1) * 14 + 28}px` }}
        >
          {t("folderLoadFailed")}
        </Text>
      ) : null}
      {open
        ? childState?.nodes.map((node) => renderChildren(node, depth + 1))
        : null}
    </Box>
  );
}

function TreeIcon({ item }: { item: BrowserNode }) {
  if (!item.available) {
    return <WarningCircle aria-hidden size={16} />;
  }
  if (item.is_network_mount) {
    return <WifiHigh aria-hidden size={16} />;
  }
  if (item.kind === "symlink" || item.kind === "junction_or_reparse_point") {
    return <Link aria-hidden size={16} />;
  }
  if (item.is_mount_point) {
    return <HardDrive aria-hidden size={16} />;
  }
  return <Folder aria-hidden size={16} />;
}

function TreeIndicators({ item }: { item: BrowserNode }) {
  const { t } = useI18n();
  const label = !item.available
    ? t("folderUnavailable")
    : item.is_network_mount
      ? t("networkFolder")
      : item.kind === "symlink" || item.kind === "junction_or_reparse_point"
        ? t("linkedFolder")
        : item.is_mount_point
          ? t("mountPoint")
          : null;
  return label ? (
    <Badge className={classes.treeBadge} color="gray" size="xs" variant="light">
      {label}
    </Badge>
  ) : null;
}

function rootItem(root: BrowserRoot): BrowserNode {
  return {
    id: root.id,
    path: root.path,
    name: root.name,
    kind: "directory",
    is_mount_point: root.kind === "file_system",
    is_network_mount: false,
    is_platform_special: false,
    available: true,
  };
}
