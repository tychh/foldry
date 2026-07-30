import {
  ActionIcon,
  Badge,
  Box,
  Button,
  Divider,
  Group,
  Loader,
  Paper,
  ScrollArea,
  SegmentedControl,
  Stack,
  Text,
  TextInput,
  Tooltip,
} from "@mantine/core";
import {
  ArrowLeft,
  CaretDown,
  CaretRight,
  File,
  Folder,
  Footprints,
  HardDrive,
  Link,
  List,
  Minus,
  Plus,
  Sigma,
  Star,
  TreeStructure,
  WarningCircle,
  WifiHigh,
  X,
} from "@phosphor-icons/react";
import {
  type HTMLAttributes,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type {
  BrowserChildren,
  BrowserNode,
  BrowserRoot,
  BrowserSize,
  BrowserView,
} from "../../shared/contracts/generated";
import { useI18n } from "../../shared/i18n/I18nProvider";
import { useDesktopData } from "../../shared/ipc/DesktopDataProvider";
import { basename } from "./folderModel";
import {
  adjacentSelection,
  appendBounded,
  edgeSelection,
  flattenTreeRows,
  formatIec,
} from "./folderBrowserModel";
import classes from "./FoldersWorkspace.module.css";

const PAGE_SIZE = 250;
const MAX_LOADED_NODES = 500;

export type FolderBrowserMode =
  | {
      type: "multi-toggle-folders";
      addedPaths: ReadonlySet<string>;
      onToggle: (path: string, added: boolean) => Promise<void>;
    }
  | {
      type: "single-directory";
      sourcePath: string;
      onConfirm: (path: string) => void;
    };

type FolderBrowserProps = {
  roots: BrowserRoot[];
  initialPath?: string;
  mode: FolderBrowserMode;
  view: BrowserView;
  onClose: () => void;
  onViewChange: (view: BrowserView) => void;
};

type DirectoryPage = {
  status: "loading" | "loaded" | "error";
  nodes: BrowserNode[];
  nextCursor: string | null;
  total: bigint;
};

type SizeState =
  | { status: "loading" }
  | { status: "ready"; result: BrowserSize }
  | { status: "error" };

type BrowserSection = "location" | "favorite" | "recent";

type ContextMenuState = {
  x: number;
  y: number;
  path: string;
  section: Exclude<BrowserSection, "location">;
} | null;

export function FolderBrowser({
  roots,
  initialPath,
  mode,
  view,
  onClose,
  onViewChange,
}: FolderBrowserProps) {
  const { t } = useI18n();
  const { query } = useDesktopData();
  const initialRoot = useMemo(
    () => rootForPath(roots, initialPath) ?? roots[0] ?? null,
    [initialPath, roots],
  );
  const [activeRoot, setActiveRoot] = useState<BrowserRoot | null>(initialRoot);
  const [selectedPath, setSelectedPath] = useState(
    initialPath ?? initialRoot?.path ?? "",
  );
  const [currentDirectory, setCurrentDirectory] = useState(
    initialPath ?? initialRoot?.path ?? "",
  );
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [pages, setPages] = useState<ReadonlyMap<string, DirectoryPage>>(
    () => new Map(),
  );
  const [sizes, setSizes] = useState<ReadonlyMap<string, SizeState>>(
    () => new Map(),
  );
  const [favorites, setFavorites] = useState<string[]>([]);
  const [favoriteNodes, setFavoriteNodes] = useState<BrowserNode[]>([]);
  const [recentNodes, setRecentNodes] = useState<BrowserNode[]>([]);
  const [manualPath, setManualPath] = useState("");
  const [manualError, setManualError] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<ContextMenuState>(null);
  const requestVersions = useRef(new Map<string, number>());
  const revealingPaths = useRef(new Map<string, number>());
  const revealVersion = useRef(0);
  const rowRefs = useRef(new Map<string, HTMLElement>());
  const sizesRef = useRef(sizes);

  useEffect(() => {
    sizesRef.current = sizes;
  }, [sizes]);

  const resolveNodes = useCallback(
    async (paths: string[]) =>
      Promise.all(
        paths.map((path) => query<BrowserNode>("browser_node", { path })),
      ),
    [query],
  );

  useEffect(() => {
    let active = true;
    void Promise.all([
      query<string[]>("browser_favorites"),
      query<string[]>("browser_recent"),
    ]).then(async ([nextFavorites, nextRecent]) => {
      const [nextFavoriteNodes, nextRecentNodes] = await Promise.all([
        resolveNodes(nextFavorites),
        resolveNodes(nextRecent),
      ]);
      if (!active) return;
      setFavorites(nextFavorites);
      setFavoriteNodes(nextFavoriteNodes);
      setRecentNodes(nextRecentNodes);
    });
    return () => {
      active = false;
    };
  }, [query, resolveNodes]);

  const loadPage = useCallback(
    async (path: string, append = false) => {
      const previous = pages.get(path);
      const cursor = append ? previous?.nextCursor : null;
      if (append && !cursor) return;
      const version = (requestVersions.current.get(path) ?? 0) + 1;
      requestVersions.current.set(path, version);
      setPages((current) => {
        const next = new Map(current);
        next.set(path, {
          status: "loading",
          nodes: append ? (current.get(path)?.nodes ?? []) : [],
          nextCursor: cursor ?? null,
          total: current.get(path)?.total ?? 0n,
        });
        return next;
      });
      try {
        const response = await query<BrowserChildren>("browser_children", {
          path,
          cursor,
          limit: PAGE_SIZE,
        });
        if (requestVersions.current.get(path) !== version) return;
        setPages((current) => {
          const next = new Map(current);
          const existing = append ? (current.get(path)?.nodes ?? []) : [];
          const nodes = appendBounded(
            existing,
            response.nodes,
            MAX_LOADED_NODES,
          );
          next.set(path, {
            status: "loaded",
            nodes,
            nextCursor: response.next_cursor,
            total: response.total,
          });
          return next;
        });
      } catch {
        if (requestVersions.current.get(path) !== version) return;
        setPages((current) => {
          const next = new Map(current);
          next.set(path, {
            status: "error",
            nodes: append ? (current.get(path)?.nodes ?? []) : [],
            nextCursor: null,
            total: current.get(path)?.total ?? 0n,
          });
          return next;
        });
      }
    },
    [pages, query],
  );

  const revealTreePath = useCallback(
    async (root: BrowserRoot, path: string, expandTarget: boolean) => {
      const version = revealVersion.current + 1;
      revealVersion.current = version;
      const paths = ancestorDirectories(root.path, path);
      if (expandTarget) paths.push(path);

      for (const currentPath of paths) {
        revealingPaths.current.set(currentPath, version);
        try {
          const response = await query<BrowserChildren>("browser_children", {
            path: currentPath,
            cursor: null,
            limit: PAGE_SIZE,
          });
          if (revealVersion.current !== version) return;
          setPages((current) => {
            const next = new Map(current);
            next.set(currentPath, {
              status: "loaded",
              nodes: response.nodes,
              nextCursor: response.next_cursor,
              total: response.total,
            });
            return next;
          });
          setExpanded((current) => new Set(current).add(currentPath));
        } catch {
          if (revealVersion.current !== version) return;
          setPages((current) => {
            const next = new Map(current);
            next.set(currentPath, {
              status: "error",
              nodes: [],
              nextCursor: null,
              total: 0n,
            });
            return next;
          });
        } finally {
          if (revealingPaths.current.get(currentPath) === version) {
            revealingPaths.current.delete(currentPath);
          }
        }
      }
    },
    [query],
  );

  useEffect(() => {
    if (
      !initialPath ||
      !activeRoot ||
      initialPath === activeRoot.path ||
      !isSameOrInside(initialPath, activeRoot.path)
    )
      return;
    queueMicrotask(() => void revealTreePath(activeRoot, initialPath, false));
  }, [activeRoot, initialPath, revealTreePath]);

  useEffect(
    () => () => {
      revealVersion.current += 1;
      revealingPaths.current.clear();
    },
    [],
  );

  useEffect(() => {
    if (!activeRoot) return;
    const path = view === "tree" ? activeRoot.path : currentDirectory;
    if (path && !pages.has(path) && !revealingPaths.current.has(path)) {
      queueMicrotask(() => void loadPage(path));
    }
  }, [activeRoot, currentDirectory, loadPage, pages, view]);

  useEffect(() => {
    const requests = requestVersions.current;
    return () => {
      for (const path of requests.keys()) {
        void query<boolean>("cancel_browser_request", { path }).catch(
          () => undefined,
        );
      }
      for (const [path, state] of sizesRef.current) {
        if (state.status === "loading") {
          void query<boolean>("cancel_browser_size", { path }).catch(
            () => undefined,
          );
        }
      }
    };
  }, [query]);

  useEffect(() => {
    const row = rowRefs.current.get(selectedPath);
    row?.scrollIntoView({ block: "nearest" });
    row?.focus({ preventScroll: true });
  }, [pages, selectedPath, view]);

  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    document.addEventListener("pointerdown", close);
    return () => {
      document.removeEventListener("pointerdown", close);
    };
  }, [contextMenu]);

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (contextMenu) setContextMenu(null);
      else onClose();
    };
    document.addEventListener("keydown", keydown);
    return () => document.removeEventListener("keydown", keydown);
  }, [contextMenu, onClose]);

  const toggleExpanded = useCallback(
    async (path: string) => {
      if (expanded.has(path)) {
        setExpanded((current) => without(current, path));
        void query<boolean>("cancel_browser_request", { path }).catch(
          () => undefined,
        );
        return;
      }
      setExpanded((current) => new Set(current).add(path));
      if (!pages.has(path)) await loadPage(path);
    },
    [expanded, loadPage, pages, query],
  );

  const treeRows = useMemo(() => {
    if (!activeRoot) return [];
    return flattenTreeRows(rootNode(activeRoot), expanded, pages);
  }, [activeRoot, expanded, pages]);

  const listPage = pages.get(currentDirectory);
  const listDirectories = (listPage?.nodes ?? []).filter(isSelectableDirectory);
  const selectedNode =
    treeRows.find((entry) => entry.node.path === selectedPath)?.node ??
    listDirectories.find((node) => node.path === selectedPath) ??
    favoriteNodes.find((node) => node.path === selectedPath) ??
    recentNodes.find((node) => node.path === selectedPath) ??
    roots.map(rootNode).find((node) => node.path === selectedPath) ??
    null;
  const selectedIsFavorite = favorites.includes(selectedPath);
  const invalidOutput =
    mode.type === "single-directory" &&
    isSameOrInside(selectedPath, mode.sourcePath);
  const canUseSelected =
    selectedNode?.available === true &&
    selectedNode.kind === "directory" &&
    !invalidOutput;

  const cancelPendingSizes = () => {
    const loading = [...sizes].filter(
      ([, state]) => state.status === "loading",
    );
    if (!loading.length) return;
    for (const [path] of loading) {
      void query<boolean>("cancel_browser_size", { path }).catch(() => false);
    }
    setSizes((current) => {
      const next = new Map(current);
      for (const [path] of loading) next.set(path, { status: "error" });
      return next;
    });
  };

  const selectSectionPath = (
    path: string,
    section: BrowserSection,
    node?: BrowserNode,
  ) => {
    cancelPendingSizes();
    const root = rootForPath(roots, path) ?? activeRoot;
    if (root) setActiveRoot(root);
    setSelectedPath(path);
    setCurrentDirectory(path);
    if (section !== "location" && node && !node.available) return;
    if (view === "list") {
      if (!pages.has(path)) void loadPage(path);
    } else if (root && node && isSelectableDirectory(node)) {
      void revealTreePath(root, path, true);
    }
  };

  const updateFavorite = async (path: string, favorite: boolean) => {
    try {
      const next = await query<string[]>("set_favorite", { path, favorite });
      setFavorites(next);
      setFavoriteNodes(await resolveNodes(next));
      setManualError(null);
    } catch {
      setManualError(t("favoritePathInvalid"));
    }
  };

  const toggleAdded = async (path: string) => {
    if (mode.type !== "multi-toggle-folders") return;
    await mode.onToggle(path, mode.addedPaths.has(path));
  };

  const calculateSize = async (path: string) => {
    const current = sizes.get(path);
    if (current?.status === "loading") {
      await query<boolean>("cancel_browser_size", { path }).catch(() => false);
    }
    setSizes((states) => new Map(states).set(path, { status: "loading" }));
    try {
      const result = await query<BrowserSize>("browser_size", { path });
      setSizes((states) =>
        new Map(states).set(path, { status: "ready", result }),
      );
    } catch {
      setSizes((states) => new Map(states).set(path, { status: "error" }));
    }
  };

  const handleTreeKey = (event: React.KeyboardEvent) => {
    if (isTextInput(event.target)) return;
    const selectable = treeRows.filter((row) =>
      isSelectableDirectory(row.node),
    );
    const index = selectable.findIndex((row) => row.node.path === selectedPath);
    const current = selectable[Math.max(index, 0)];
    if (!current) return;
    const moveTo = (next: (typeof selectable)[number] | undefined) => {
      if (next) setSelectedPath(next.node.path);
    };
    switch (event.key) {
      case "ArrowDown":
        moveTo(
          selectable.find(
            (row) =>
              row.node.path ===
              adjacentSelection(
                selectable.map((candidate) => candidate.node.path),
                current.node.path,
                1,
              ),
          ),
        );
        break;
      case "ArrowUp":
        moveTo(
          selectable.find(
            (row) =>
              row.node.path ===
              adjacentSelection(
                selectable.map((candidate) => candidate.node.path),
                current.node.path,
                -1,
              ),
          ),
        );
        break;
      case "ArrowRight":
      case "Enter":
        void toggleExpanded(current.node.path);
        break;
      case "ArrowLeft": {
        if (expanded.has(current.node.path)) {
          void toggleExpanded(current.node.path);
        } else {
          const parent = parentPath(current.node.path);
          moveTo(selectable.find((row) => row.node.path === parent));
        }
        break;
      }
      case " ":
        void toggleAdded(current.node.path);
        break;
      case "?":
      case "=":
        void calculateSize(current.node.path);
        break;
      case "Home":
      case "End": {
        const parent = parentPath(current.node.path);
        const siblings = selectable.filter(
          (row) => parentPath(row.node.path) === parent,
        );
        const edge = edgeSelection(
          siblings.map((candidate) => candidate.node.path),
          event.key === "Home" ? "first" : "last",
        );
        moveTo(siblings.find((row) => row.node.path === edge));
        break;
      }
      default:
        if (event.code === "NumpadAdd") {
          if (!expanded.has(current.node.path))
            void toggleExpanded(current.node.path);
        } else if (event.code === "NumpadSubtract") {
          if (expanded.has(current.node.path))
            void toggleExpanded(current.node.path);
          else {
            const parent = parentPath(current.node.path);
            moveTo(selectable.find((row) => row.node.path === parent));
          }
        } else {
          return;
        }
    }
    event.preventDefault();
  };

  const handleListKey = (event: React.KeyboardEvent) => {
    if (isTextInput(event.target)) return;
    const parent = parentPath(currentDirectory);
    const selectable = [
      ...(parent ? [previewParentNode(parent)] : []),
      ...listDirectories,
    ];
    const index = Math.max(
      selectable.findIndex((node) => node.path === selectedPath),
      0,
    );
    const current = selectable[index];
    if (!current) return;
    const moveTo = (node: BrowserNode | undefined) => {
      if (node) setSelectedPath(node.path);
    };
    switch (event.key) {
      case "ArrowDown":
        moveTo(
          selectable.find(
            (node) =>
              node.path ===
              adjacentSelection(
                selectable.map((candidate) => candidate.path),
                current.path,
                1,
              ),
          ),
        );
        break;
      case "ArrowUp":
        moveTo(
          selectable.find(
            (node) =>
              node.path ===
              adjacentSelection(
                selectable.map((candidate) => candidate.path),
                current.path,
                -1,
              ),
          ),
        );
        break;
      case "Home":
        moveTo(
          selectable.find(
            (node) =>
              node.path ===
              edgeSelection(
                selectable.map((candidate) => candidate.path),
                "first",
              ),
          ),
        );
        break;
      case "End":
        moveTo(
          selectable.find(
            (node) =>
              node.path ===
              edgeSelection(
                selectable.map((candidate) => candidate.path),
                "last",
              ),
          ),
        );
        break;
      case "ArrowLeft":
        if (parent) leaveDirectory();
        break;
      case "ArrowRight":
      case "Enter":
        if (current.path === parent) leaveDirectory();
        else enterDirectory(current.path);
        break;
      case " ":
        void toggleAdded(current.path);
        break;
      case "?":
      case "=":
        void calculateSize(current.path);
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  const enterDirectory = (path: string) => {
    cancelPendingSizes();
    setCurrentDirectory(path);
    setSelectedPath(parentPath(path) ?? path);
    if (!pages.has(path)) void loadPage(path);
  };
  const leaveDirectory = () => {
    const parent = parentPath(currentDirectory);
    if (!parent) return;
    cancelPendingSizes();
    const previous = currentDirectory;
    setCurrentDirectory(parent);
    setSelectedPath(previous);
    if (!pages.has(parent)) void loadPage(parent);
  };

  return (
    <aside
      aria-label={t("folderBrowser")}
      className={classes.browserPanel}
      data-testid="folder-browser"
    >
      <header className={classes.browserHeader}>
        <Box miw={0}>
          <Text fw={750} size="lg">
            {mode.type === "multi-toggle-folders"
              ? t("addFolders")
              : t("chooseOutputFolder")}
          </Text>
          <Text className={classes.browserPath} c="dimmed" size="xs">
            <span aria-label={selectedPath} title={selectedPath}>
              {selectedPath || "—"}
            </span>
          </Text>
        </Box>
        <ActionIcon
          aria-label={t("close")}
          size="lg"
          variant="subtle"
          onClick={onClose}
        >
          <X aria-hidden size={19} />
        </ActionIcon>
      </header>

      <div className={classes.browserToolbar}>
        <Tooltip
          label={
            selectedIsFavorite ? t("removeFromFavorites") : t("addToFavorites")
          }
        >
          <ActionIcon
            aria-label={
              selectedIsFavorite
                ? t("removeFromFavorites")
                : t("addToFavorites")
            }
            disabled={!selectedNode || !selectedNode.available}
            variant={selectedIsFavorite ? "filled" : "default"}
            onClick={() =>
              void updateFavorite(selectedPath, !selectedIsFavorite)
            }
          >
            <Star
              aria-hidden
              size={17}
              weight={selectedIsFavorite ? "fill" : "regular"}
            />
          </ActionIcon>
        </Tooltip>
        <TextInput
          aria-label={t("favoritePath")}
          error={manualError}
          leftSection={<Footprints aria-hidden size={15} />}
          placeholder={t("favoritePath")}
          value={manualPath}
          onChange={(event) => setManualPath(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && manualPath.trim()) {
              void updateFavorite(manualPath.trim(), true).then(() =>
                setManualPath(""),
              );
            }
          }}
        />
        <Button
          disabled={!manualPath.trim()}
          size="compact-sm"
          variant="default"
          onClick={() =>
            void updateFavorite(manualPath.trim(), true).then(() =>
              setManualPath(""),
            )
          }
        >
          {t("pin")}
        </Button>
        <SegmentedControl
          aria-label={t("browserView")}
          data={[
            {
              label: (
                <Group gap={5} wrap="nowrap">
                  <TreeStructure aria-hidden size={15} />
                  <span>{t("treeView")}</span>
                </Group>
              ),
              value: "tree",
            },
            {
              label: (
                <Group gap={5} wrap="nowrap">
                  <List aria-hidden size={15} />
                  <span>{t("listView")}</span>
                </Group>
              ),
              value: "list",
            },
          ]}
          value={view}
          onChange={(value) => {
            const next = value as BrowserView;
            onViewChange(next);
            if (next === "list") {
              const nextDirectory =
                selectedNode?.kind === "directory"
                  ? (parentPath(selectedNode.path) ?? selectedNode.path)
                  : currentDirectory;
              setCurrentDirectory(nextDirectory);
            }
          }}
        />
      </div>

      <div className={classes.browserBody}>
        <ScrollArea className={classes.browserSidebar} offsetScrollbars>
          <BrowserSectionList
            label={t("locations")}
            nodes={roots.map(rootNode)}
            onContextMenu={undefined}
            onSelect={(node) => selectSectionPath(node.path, "location", node)}
            selectedPath={selectedPath}
          />
          <BrowserSectionList
            emptyLabel={t("noFavorites")}
            label={t("favorites")}
            nodes={favoriteNodes}
            onContextMenu={(position, node) => {
              setContextMenu({
                x: position.x,
                y: position.y,
                path: node.path,
                section: "favorite",
              });
            }}
            onSelect={(node) => selectSectionPath(node.path, "favorite", node)}
            selectedPath={selectedPath}
          />
          <BrowserSectionList
            emptyLabel={t("noRecent")}
            label={t("recent")}
            nodes={recentNodes}
            onContextMenu={(position, node) => {
              setContextMenu({
                x: position.x,
                y: position.y,
                path: node.path,
                section: "recent",
              });
            }}
            onSelect={(node) => selectSectionPath(node.path, "recent", node)}
            selectedPath={selectedPath}
            withDivider={false}
          />
        </ScrollArea>

        {view === "tree" ? (
          <ScrollArea
            className={classes.browserContent}
            viewportProps={
              {
                role: "tree",
                "aria-label": t("folderTree"),
                "data-autofocus": true,
                tabIndex: -1,
                onKeyDown: handleTreeKey,
              } as HTMLAttributes<HTMLDivElement>
            }
          >
            <Stack gap={1} p="xs">
              {treeRows.map(({ node, depth }) => {
                const open = expanded.has(node.path);
                return (
                  <BrowserRow
                    key={node.id}
                    added={
                      mode.type === "multi-toggle-folders" &&
                      mode.addedPaths.has(node.path)
                    }
                    depth={depth}
                    expanded={open}
                    node={node}
                    selected={selectedPath === node.path}
                    size={sizes.get(node.path)}
                    toggleable={mode.type === "multi-toggle-folders"}
                    onCalculateSize={() => void calculateSize(node.path)}
                    onEnter={() => {
                      setSelectedPath(node.path);
                      if (isSelectableDirectory(node))
                        void toggleExpanded(node.path);
                    }}
                    onRef={(element) => setRowRef(rowRefs, node.path, element)}
                    onToggle={() => {
                      setSelectedPath(node.path);
                      void toggleExpanded(node.path);
                    }}
                    onToggleAdded={() => void toggleAdded(node.path)}
                  />
                );
              })}
              {treeRows.some(
                ({ node }) => pages.get(node.path)?.status === "error",
              ) ? (
                <Text c="red" p="xs" size="xs">
                  {t("folderLoadFailed")}
                </Text>
              ) : null}
            </Stack>
          </ScrollArea>
        ) : (
          <div
            aria-label={t("folderList")}
            className={`${classes.browserContent} ${classes.browserListContent}`}
            role="grid"
          >
            <div className={classes.listContext} title={currentDirectory}>
              {currentDirectory}
            </div>
            <div className={classes.listHeader} role="row">
              <Text fw={650} role="columnheader" size="xs">
                {t("name")}
              </Text>
              <Text fw={650} role="columnheader" size="xs">
                {t("modified")}
              </Text>
              <Text fw={650} role="columnheader" size="xs">
                {t("size")}
              </Text>
            </div>
            <ScrollArea
              className={classes.browserListRows}
              viewportProps={
                {
                  role: "rowgroup",
                  "data-autofocus": true,
                  tabIndex: -1,
                  onKeyDown: handleListKey,
                } as HTMLAttributes<HTMLDivElement>
              }
            >
              {parentPath(currentDirectory) ? (
                <ListRow
                  isParent
                  node={previewParentNode(parentPath(currentDirectory)!)}
                  selected={selectedPath === parentPath(currentDirectory)}
                  size={undefined}
                  onCalculateSize={() => undefined}
                  onEnter={leaveDirectory}
                  onRef={(element) =>
                    setRowRef(
                      rowRefs,
                      parentPath(currentDirectory) ?? "",
                      element,
                    )
                  }
                  onSelect={() =>
                    setSelectedPath(parentPath(currentDirectory) ?? "")
                  }
                  onToggleAdded={() =>
                    void toggleAdded(parentPath(currentDirectory) ?? "")
                  }
                />
              ) : null}
              {(listPage?.nodes ?? []).map((node) => (
                <ListRow
                  key={node.id}
                  added={
                    mode.type === "multi-toggle-folders" &&
                    mode.addedPaths.has(node.path)
                  }
                  node={node}
                  selected={selectedPath === node.path}
                  size={sizes.get(node.path)}
                  toggleable={mode.type === "multi-toggle-folders"}
                  onCalculateSize={() => void calculateSize(node.path)}
                  onEnter={() =>
                    isSelectableDirectory(node) && enterDirectory(node.path)
                  }
                  onRef={(element) => setRowRef(rowRefs, node.path, element)}
                  onSelect={() =>
                    isSelectableDirectory(node) && setSelectedPath(node.path)
                  }
                  onToggleAdded={() => void toggleAdded(node.path)}
                />
              ))}
              {listPage?.status === "loading" ? (
                <Group justify="center" p="md">
                  <Loader size="sm" />
                </Group>
              ) : null}
              {listPage?.status === "error" ? (
                <Text c="red" p="md" size="sm">
                  {t("folderLoadFailed")}
                </Text>
              ) : null}
              {listPage?.nextCursor ? (
                <Group justify="center" p="md">
                  <Button
                    size="xs"
                    variant="default"
                    onClick={() => void loadPage(currentDirectory, true)}
                  >
                    {t("loadMore")}
                  </Button>
                </Group>
              ) : null}
            </ScrollArea>
          </div>
        )}
      </div>

      <footer className={classes.browserFooter}>
        <Text c={invalidOutput ? "red" : "dimmed"} size="xs">
          {invalidOutput ? t("outputInsideSource") : t("browserKeyboardHint")}
        </Text>
        {mode.type === "single-directory" ? (
          <Group gap="sm" wrap="nowrap">
            <Button
              variant="default"
              onClick={() => mode.onConfirm(parentPath(mode.sourcePath) ?? "")}
            >
              {t("useParentFolder")}
            </Button>
            <Button variant="default" onClick={onClose}>
              {t("cancel")}
            </Button>
            <Button
              disabled={!canUseSelected}
              onClick={() => mode.onConfirm(selectedPath)}
            >
              {t("useSelectedFolder")}
            </Button>
          </Group>
        ) : null}
      </footer>

      {contextMenu ? (
        <div
          className={classes.browserContextMenu}
          role="menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onPointerDown={(event) => event.stopPropagation()}
        >
          <button
            role="menuitem"
            type="button"
            onClick={() => {
              void updateFavorite(
                contextMenu.path,
                contextMenu.section === "recent",
              );
              setContextMenu(null);
            }}
          >
            {contextMenu.section === "favorite"
              ? t("removeFromFavorites")
              : t("addToFavorites")}
          </button>
        </div>
      ) : null}
    </aside>
  );
}

function BrowserSectionList({
  label,
  emptyLabel,
  nodes,
  selectedPath,
  onSelect,
  onContextMenu,
  withDivider = true,
}: {
  label: string;
  emptyLabel?: string;
  nodes: BrowserNode[];
  selectedPath: string;
  onSelect: (node: BrowserNode) => void;
  onContextMenu:
    | ((position: { x: number; y: number }, node: BrowserNode) => void)
    | undefined;
  withDivider?: boolean;
}) {
  const { t } = useI18n();
  return (
    <Box mb="md">
      <Text c="dimmed" fw={700} mb={5} size="xs">
        {label}
      </Text>
      <Stack gap={2}>
        {nodes.map((node) => (
          <button
            key={`${label}-${node.id}`}
            className={classes.browserSectionItem}
            data-selected={selectedPath === node.path || undefined}
            title={node.path}
            type="button"
            onClick={() => onSelect(node)}
            onContextMenu={(event) => {
              event.preventDefault();
              onContextMenu?.({ x: event.clientX, y: event.clientY }, node);
            }}
            onKeyDown={(event) => {
              if (
                onContextMenu &&
                (event.key === "ContextMenu" ||
                  (event.shiftKey && event.key === "F10"))
              ) {
                const bounds = event.currentTarget.getBoundingClientRect();
                onContextMenu(
                  { x: bounds.left + 16, y: bounds.bottom - 2 },
                  node,
                );
                event.preventDefault();
              }
            }}
          >
            <NodeIcon node={node} />
            <span>{node.name}</span>
            {!node.available ? (
              <Badge color="gray" size="xs" variant="light">
                {t("folderUnavailable")}
              </Badge>
            ) : null}
          </button>
        ))}
        {!nodes.length && emptyLabel ? (
          <Text c="dimmed" px="xs" size="xs">
            {emptyLabel}
          </Text>
        ) : null}
      </Stack>
      {withDivider ? <Divider mt="md" /> : null}
    </Box>
  );
}

function BrowserRow({
  node,
  depth,
  selected,
  expanded,
  added = false,
  size,
  toggleable,
  onEnter,
  onToggle,
  onToggleAdded,
  onCalculateSize,
  onRef,
}: {
  node: BrowserNode;
  depth: number;
  selected: boolean;
  expanded: boolean;
  added?: boolean;
  size: SizeState | undefined;
  toggleable: boolean;
  onEnter: () => void;
  onToggle: () => void;
  onToggleAdded: () => void;
  onCalculateSize: () => void;
  onRef: (element: HTMLElement | null) => void;
}) {
  const { t } = useI18n();
  const selectable = isSelectableDirectory(node);
  return (
    <Paper
      ref={onRef}
      aria-selected={selected}
      className={classes.browserRow}
      data-file={node.kind !== "directory" || undefined}
      data-selected={selected || undefined}
      data-unavailable={!node.available || undefined}
      role="treeitem"
      style={{ paddingInlineStart: `${depth * 16 + 4}px` }}
      tabIndex={selected ? 0 : -1}
      withBorder={false}
    >
      {node.kind === "directory" ? (
        <ActionIcon
          aria-label={expanded ? t("collapseFolder") : t("expandFolder")}
          disabled={!selectable}
          size={24}
          tabIndex={-1}
          variant="subtle"
          onClick={(event) => {
            event.stopPropagation();
            onToggle();
          }}
        >
          {expanded ? (
            <CaretDown aria-hidden size={13} />
          ) : (
            <CaretRight aria-hidden size={13} />
          )}
        </ActionIcon>
      ) : (
        <span aria-hidden className={classes.browserRowCaretSpacer} />
      )}
      <button
        className={classes.browserRowName}
        disabled={!selectable}
        title={node.path}
        type="button"
        tabIndex={-1}
        onClick={onEnter}
      >
        <NodeIcon node={node} />
        <span>{node.name}</span>
      </button>
      <NodeState node={node} />
      <SizeBadge size={size} />
      {selectable ? (
        <>
          <ActionIcon
            aria-label={t("calculateSize")}
            className={classes.browserRowAction}
            size={24}
            tabIndex={-1}
            variant="subtle"
            onClick={onCalculateSize}
          >
            <Sigma aria-hidden size={13} />
          </ActionIcon>
          {toggleable ? (
            <ActionIcon
              aria-label={added ? t("removeFromFolders") : t("addFolder")}
              className={classes.browserRowAction}
              color={added ? "red" : "blue"}
              size={24}
              tabIndex={-1}
              variant="subtle"
              onClick={onToggleAdded}
            >
              {added ? (
                <Minus aria-hidden size={14} />
              ) : (
                <Plus aria-hidden size={14} />
              )}
            </ActionIcon>
          ) : null}
          {toggleable && added ? (
            <Badge color="blue" size="xs" variant="light">
              {t("added")}
            </Badge>
          ) : null}
        </>
      ) : null}
    </Paper>
  );
}

function ListRow({
  node,
  selected,
  added = false,
  isParent = false,
  size,
  toggleable = false,
  onSelect,
  onEnter,
  onToggleAdded,
  onCalculateSize,
  onRef,
}: {
  node: BrowserNode;
  selected: boolean;
  added?: boolean;
  isParent?: boolean;
  size: SizeState | undefined;
  toggleable?: boolean;
  onSelect: () => void;
  onEnter: () => void;
  onToggleAdded: () => void;
  onCalculateSize: () => void;
  onRef: (element: HTMLElement | null) => void;
}) {
  const { t } = useI18n();
  const selectable = isSelectableDirectory(node);
  return (
    <div
      ref={onRef}
      aria-selected={selected}
      className={classes.listRow}
      data-file={node.kind !== "directory" || undefined}
      data-selected={selected || undefined}
      data-unavailable={!node.available || undefined}
      role="row"
      tabIndex={selected ? 0 : -1}
      onClick={onSelect}
      onDoubleClick={onEnter}
    >
      <div className={classes.listName} role="gridcell">
        {isParent ? (
          <ArrowLeft aria-hidden size={16} />
        ) : (
          <NodeIcon node={node} />
        )}
        <span>{isParent ? ".." : node.name}</span>
        {added ? (
          <Badge color="blue" size="xs" variant="light">
            {t("added")}
          </Badge>
        ) : null}
      </div>
      <Text c="dimmed" role="gridcell" size="xs">
        {isParent ? "" : formatModified(node.modified_at_unix_ms)}
      </Text>
      <Group gap={4} justify="flex-end" role="gridcell" wrap="nowrap">
        {selectable && !isParent ? (
          <>
            <ListSize size={size} />
            <ActionIcon
              aria-label={t("calculateSize")}
              className={classes.browserRowAction}
              size="sm"
              tabIndex={-1}
              variant="subtle"
              onClick={(event) => {
                event.stopPropagation();
                onCalculateSize();
              }}
            >
              <Sigma aria-hidden size={13} />
            </ActionIcon>
            {toggleable ? (
              <ActionIcon
                aria-label={added ? t("removeFromFolders") : t("addFolder")}
                className={classes.browserRowAction}
                color={added ? "red" : "blue"}
                size="sm"
                tabIndex={-1}
                variant="subtle"
                onClick={(event) => {
                  event.stopPropagation();
                  onToggleAdded();
                }}
              >
                {added ? (
                  <Minus aria-hidden size={14} />
                ) : (
                  <Plus aria-hidden size={14} />
                )}
              </ActionIcon>
            ) : null}
          </>
        ) : null}
      </Group>
    </div>
  );
}

function NodeIcon({ node }: { node: BrowserNode }) {
  if (!node.available) return <WarningCircle aria-hidden size={16} />;
  if (node.is_network_mount) return <WifiHigh aria-hidden size={16} />;
  if (node.kind === "symlink" || node.kind === "junction_or_reparse_point")
    return <Link aria-hidden size={16} />;
  if (node.is_mount_point) return <HardDrive aria-hidden size={16} />;
  if (node.kind !== "directory") return <File aria-hidden size={16} />;
  return <Folder aria-hidden size={16} />;
}

function NodeState({ node }: { node: BrowserNode }) {
  const { t } = useI18n();
  const label = !node.available
    ? t("folderUnavailable")
    : node.is_network_mount
      ? t("networkFolder")
      : node.kind === "symlink" || node.kind === "junction_or_reparse_point"
        ? t("linkedFolder")
        : node.is_mount_point
          ? t("mountPoint")
          : null;
  return label ? (
    <Badge color="gray" size="xs" variant="light">
      {label}
    </Badge>
  ) : null;
}

function SizeBadge({ size }: { size: SizeState | undefined }) {
  const { t } = useI18n();
  if (!size) return null;
  if (size.status === "loading")
    return <Loader aria-label={t("calculating")} size={13} />;
  if (size.status === "error")
    return (
      <Badge color="red" size="xs" variant="light">
        {t("sizeFailed")}
      </Badge>
    );
  const label = formatIec(BigInt(size.result.logical_bytes));
  return (
    <Badge
      color={size.result.partial ? "yellow" : "gray"}
      size="xs"
      title={
        size.result.partial
          ? t("partialSizeHint", { count: Number(size.result.warnings) })
          : label
      }
      variant="light"
    >
      {size.result.partial ? `${label}*` : label}
    </Badge>
  );
}

function ListSize({ size }: { size: SizeState | undefined }) {
  const { t } = useI18n();
  if (!size) return null;
  if (size.status === "loading")
    return <Loader aria-label={t("calculating")} size={13} />;
  if (size.status === "error")
    return (
      <Text className={classes.listSizeValue} c="red" size="xs">
        {t("sizeFailed")}
      </Text>
    );
  const label = formatIec(BigInt(size.result.logical_bytes));
  return (
    <Text
      className={classes.listSizeValue}
      c={size.result.partial ? "yellow.6" : "dimmed"}
      size="xs"
      title={
        size.result.partial
          ? t("partialSizeHint", { count: Number(size.result.warnings) })
          : label
      }
    >
      {size.result.partial ? `${label}*` : label}
    </Text>
  );
}

function rootNode(root: BrowserRoot): BrowserNode {
  return {
    id: root.id,
    path: root.path,
    name: root.name,
    kind: "directory",
    is_mount_point: root.kind === "drive" || root.kind === "file_system",
    is_network_mount: false,
    is_platform_special: false,
    available: true,
    modified_at_unix_ms: null,
  };
}

function previewParentNode(path: string): BrowserNode {
  return {
    id: `parent:${path}`,
    path,
    name: basename(path),
    kind: "directory",
    is_mount_point: false,
    is_network_mount: false,
    is_platform_special: false,
    available: true,
    modified_at_unix_ms: null,
  };
}

function rootForPath(
  roots: BrowserRoot[],
  path: string | undefined,
): BrowserRoot | null {
  if (!path) return null;
  return (
    [...roots]
      .filter((root) => isSameOrInside(path, root.path))
      .sort((left, right) => right.path.length - left.path.length)[0] ?? null
  );
}

function isSelectableDirectory(node: BrowserNode): boolean {
  return node.kind === "directory" && node.available;
}

function parentPath(path: string): string | null {
  const windows = path.includes("\\");
  const separator = windows ? "\\" : "/";
  const normalized = path.replace(/[\\/]+$/, "");
  const index = normalized.lastIndexOf(separator);
  if (index < 0) return null;
  if (windows && index === 2) return `${normalized.slice(0, 2)}\\`;
  if (!windows && index === 0) return "/";
  return normalized.slice(0, index);
}

function isSameOrInside(path: string, parent: string): boolean {
  if (!path || !parent) return false;
  const windows = path.includes("\\") || parent.includes("\\");
  const normalize = (value: string) =>
    value
      .replaceAll("\\", "/")
      .replace(/\/+$/, "")
      .normalize("NFC")
      .toLocaleLowerCase(windows ? "en-US" : undefined);
  const normalizedPath = normalize(path);
  const normalizedParent = normalize(parent);
  return (
    normalizedPath === normalizedParent ||
    normalizedPath.startsWith(`${normalizedParent}/`)
  );
}

function ancestorDirectories(root: string, target: string): string[] {
  const result: string[] = [];
  let current = target;
  while (current !== root) {
    const parent = parentPath(current);
    if (!parent || !isSameOrInside(parent, root)) break;
    result.unshift(parent);
    current = parent;
  }
  return result;
}

function without(values: ReadonlySet<string>, value: string): Set<string> {
  const next = new Set(values);
  next.delete(value);
  return next;
}

function setRowRef(
  refs: React.RefObject<Map<string, HTMLElement>>,
  path: string,
  element: HTMLElement | null,
) {
  if (element) refs.current.set(path, element);
  else refs.current.delete(path);
}

function isTextInput(target: EventTarget): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  );
}

function formatModified(value: string | null): string {
  if (!value) return "";
  const date = new Date(Number(value));
  return Number.isNaN(date.getTime())
    ? ""
    : new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
      }).format(date);
}
