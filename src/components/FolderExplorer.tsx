import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import ContextMenu, { type ContextEntry } from "../menu/ContextMenu";
import { preview } from "../analysis/api";
import { useLibraryScan } from "./useLibraryScan";

interface DirEntry {
  name: string;
  path: string;
}

interface Track {
  id: number;
  title: string;
  artist: string;
  album: string;
  genre: string;
  duration_ms: number;
  file_path: string;
}

interface FileEntry {
  name: string;
  path: string;
  size: number;
  track: Track | null;
}

interface DirListing {
  path: string;
  parent: string | null;
  dirs: DirEntry[];
  files: FileEntry[];
}

interface LibraryFolder {
  path: string;
  track_count: number;
}

const TREE_KEY = "riars.library.tree";

const readTree = (): { expanded: string[]; selected: string | null } => {
  try {
    const raw = localStorage.getItem(TREE_KEY);
    if (raw) {
      const value = JSON.parse(raw) as { expanded?: unknown; selected?: unknown };
      return {
        expanded: Array.isArray(value.expanded)
          ? value.expanded.filter((p): p is string => typeof p === "string")
          : [],
        selected: typeof value.selected === "string" ? value.selected : null,
      };
    }
  } catch { /* storage not available */ }
  return { expanded: [], selected: null };
};

const nameWithoutExtension = (name: string) => {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
};
// The last part of a path and what comes before it, for the narrow folder list.
const splitPath = (path: string) => {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  return cut < 0 ? { base: path, parent: "" } : { base: trimmed.slice(cut + 1), parent: trimmed.slice(0, cut + 1) };
};
const baseName = (path: string) => splitPath(path).base || path;
const parentPath = (path: string) => splitPath(path).parent;
const fileTitle = (file: FileEntry) => file.track?.title ?? nameWithoutExtension(file.name);
const fileArtist = (file: FileEntry) => file.track?.artist ?? "";
const fileDuration = (file: FileEntry) => file.track?.duration_ms ?? 0;

const S: Record<string, React.CSSProperties> = {
  wrap: { display: "flex", flexDirection: "column", height: "100%", minHeight: 0 },
  header: {
    display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8,
    padding: "10px 12px", borderBottom: "1px solid var(--border)",
  },
  title: { fontSize: 14, fontWeight: 600, minWidth: 0 },
  headerButtons: { display: "flex", gap: 6, flexShrink: 0 },
  btn: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)" },
  notice: { padding: "6px 12px", fontSize: 12, borderBottom: "1px solid var(--border)", wordBreak: "break-word" },
  folderList: {
    maxHeight: "30%", overflowY: "auto", borderBottom: "1px solid var(--border)", flexShrink: 0,
  },
  folderRow: {
    display: "flex", alignItems: "center", gap: 8, padding: "6px 12px",
    borderBottom: "1px solid rgba(255,255,255,0.03)", fontSize: 12,
  },
  folderPath: { flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  folderName: { display: "block", overflow: "hidden", textOverflow: "ellipsis" },
  folderParent: { display: "block", overflow: "hidden", textOverflow: "ellipsis", fontSize: 10, color: "var(--text-muted)" },
  folderCount: { color: "var(--text-muted)", fontSize: 11, flexShrink: 0 },
  removeBtn: {
    padding: "0 6px", borderRadius: 4, fontSize: 12, color: "var(--text-muted)",
    background: "transparent", flexShrink: 0,
  },
  muted: { padding: "10px 12px", color: "var(--text-muted)", fontSize: 12 },
  treeSection: { flex: 1, minHeight: 0, display: "flex", flexDirection: "column" },
  treeHeading: {
    padding: "6px 12px", fontSize: 11, color: "var(--text-muted)", borderBottom: "1px solid var(--border)",
    flexShrink: 0,
  },
  tree: { flex: 1, minHeight: 0, overflow: "auto", padding: "4px 0" },
  node: { display: "flex", alignItems: "center", gap: 4, padding: "3px 8px", fontSize: 12, minHeight: 22 },
  expander: {
    width: 18, padding: 0, background: "transparent", fontSize: 11, color: "var(--text-muted)",
    flexShrink: 0, lineHeight: 1,
  },
  checkbox: { margin: 0, flexShrink: 0 },
  nodeName: {
    flex: 1, minWidth: 0, padding: "1px 4px", borderRadius: 4, overflow: "hidden",
    textOverflow: "ellipsis", whiteSpace: "nowrap" as const, cursor: "pointer", userSelect: "none" as const,
  },
  nodeSelected: { background: "rgba(233,69,96,0.18)" },
  nodeInfo: { fontSize: 11, color: "var(--text-muted)", padding: "1px 8px 3px 0" },
  nodeError: { fontSize: 11, color: "var(--red)", padding: "1px 8px 3px 0", wordBreak: "break-word" },
  files: {
    flex: 1, minHeight: 0, overflowY: "auto", borderTop: "1px solid var(--border)", outline: "none",
  },
  fileRow: {
    display: "flex", alignItems: "center", gap: 6, padding: "7px 12px", cursor: "pointer",
    userSelect: "none" as const, borderBottom: "1px solid rgba(255,255,255,0.03)",
  },
  selected: { background: "rgba(233,69,96,0.18)" },
  dimmed: { opacity: 0.55 },
  fileBody: { flex: 1, minWidth: 0 },
  fileName: { fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  meta: {
    display: "flex", justifyContent: "space-between", fontSize: 11, color: "var(--text-muted)", marginTop: 2,
  },
  empty: { padding: "24px 12px", textAlign: "center", color: "var(--text-muted)", fontSize: 13 },
  overlay: {
    position: "fixed", inset: 0, zIndex: 1100, background: "rgba(0,0,0,.5)",
    display: "flex", alignItems: "center", justifyContent: "center",
  },
  dialog: {
    minWidth: 340, maxWidth: 460, padding: 18, background: "var(--bg-panel)",
    border: "1px solid var(--border)", borderRadius: 8, boxShadow: "0 8px 24px rgba(0,0,0,0.45)",
  },
  dialogTitle: { fontSize: 14, fontWeight: 600, marginBottom: 8 },
  dialogText: { fontSize: 12, color: "var(--text-secondary)", marginBottom: 16, lineHeight: 1.4 },
  dialogButtons: { display: "flex", justifyContent: "flex-end", gap: 8, flexWrap: "wrap" as const },
  dangerBtn: { ...{ padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12 }, color: "var(--red)" },
};

export default function FolderExplorer({ onLibraryChanged }: { onLibraryChanged: () => void }) {
  const { t } = useTranslation();
  const [initial] = useState(readTree);
  const [roots, setRoots] = useState<DirEntry[]>([]);
  const [folders, setFolders] = useState<LibraryFolder[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(initial.expanded));
  const [selected, setSelected] = useState<string | null>(initial.selected);
  const [listings, setListings] = useState<Record<string, DirListing>>({});
  const [loading, setLoading] = useState<Set<string>>(new Set());
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [selectedFiles, setSelectedFiles] = useState<Set<string>>(new Set());
  const [anchor, setAnchor] = useState<string | null>(null);
  const [ctx, setCtx] = useState<{ x: number; y: number } | null>(null);
  const [removeTarget, setRemoveTarget] = useState<string | null>(null);
  const [notice, setNotice] = useState<{ text: string; error?: boolean } | null>(null);
  const filesRef = useRef<HTMLDivElement>(null);
  const inFlight = useRef<Set<string>>(new Set());
  const selectedRef = useRef<string | null>(selected);
  selectedRef.current = selected;

  const loadFolders = useCallback(async () => {
    try {
      setFolders(await invoke<LibraryFolder[]>("library_folders"));
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  }, []);

  const loadDir = useCallback(async (path: string) => {
    if (inFlight.current.has(path)) return;
    inFlight.current.add(path);
    setLoading((prev) => new Set(prev).add(path));
    try {
      const listing = await invoke<DirListing>("fs_list_dir", { path });
      setListings((prev) => ({ ...prev, [path]: listing }));
      setErrors((prev) => {
        if (!(path in prev)) return prev;
        const next = { ...prev };
        delete next[path];
        return next;
      });
    } catch (e) {
      setErrors((prev) => ({ ...prev, [path]: String(e) }));
    } finally {
      inFlight.current.delete(path);
      setLoading((prev) => {
        if (!prev.has(path)) return prev;
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
    }
  }, []);

  // After a scan: new track counts, and files of the open folder that became tracks.
  // The Library tab reloads its own list on the same event.
  const refresh = useCallback(() => {
    void loadFolders();
    const path = selectedRef.current;
    if (path) void loadDir(path);
  }, [loadFolders, loadDir]);

  const { busy } = useLibraryScan(setNotice, refresh);

  useEffect(() => {
    invoke<DirEntry[]>("fs_roots").then(setRoots).catch((e) => setNotice({ text: String(e), error: true }));
    void loadFolders();
    for (const path of initial.expanded) void loadDir(path);
    if (initial.selected) void loadDir(initial.selected);
  }, [initial, loadFolders, loadDir]);

  useEffect(() => {
    try {
      localStorage.setItem(TREE_KEY, JSON.stringify({ expanded: [...expanded], selected }));
    } catch { /* storage not available */ }
  }, [expanded, selected]);

  useEffect(() => {
    setSelectedFiles(new Set());
    setAnchor(null);
  }, [selected]);

  useEffect(() => {
    if (!removeTarget) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setRemoveTarget(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [removeTarget]);

  const isLibraryFolder = (path: string) => folders.some((f) => f.path === path);

  const containingFolder = (path: string): string | null => {
    for (const folder of folders) {
      if (folder.path === path) continue;
      const sep = folder.path.includes("\\") ? "\\" : "/";
      const base = folder.path.endsWith("\\") || folder.path.endsWith("/") ? folder.path : folder.path + sep;
      if (path.startsWith(base)) return folder.path;
    }
    return null;
  };

  const addFolders = useCallback(async () => {
    try {
      const chosen = await open({ directory: true, multiple: true });
      if (!chosen) return;
      const paths = Array.isArray(chosen) ? chosen : [chosen];
      if (paths.length === 0) return;
      await invoke<string[]>("library_add_folders", { paths });
      await loadFolders();
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  }, [loadFolders]);

  const rescan = useCallback(async () => {
    try {
      await invoke("library_rescan");
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  }, []);

  const toggleExpand = (path: string) => {
    const willExpand = !expanded.has(path);
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
    // Reload on every expand, so folders created since show up.
    if (willExpand) void loadDir(path);
  };

  const selectFolder = (path: string) => {
    setSelected(path);
    if (!listings[path]) void loadDir(path);
  };

  const onCheck = async (path: string) => {
    if (isLibraryFolder(path)) {
      setRemoveTarget(path);
      return;
    }
    if (containingFolder(path)) return;
    try {
      await invoke<string[]>("library_add_folders", { paths: [path] });
      await loadFolders();
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  };

  const removeFolder = async (removeTracks: boolean) => {
    const path = removeTarget;
    if (!path) return;
    try {
      const count = await invoke<number>("library_remove_folder", { path, removeTracks });
      setRemoveTarget(null);
      setNotice({ text: t("library.folders.removed", { count }) });
      await loadFolders();
      onLibraryChanged();
    } catch (e) {
      setRemoveTarget(null);
      setNotice({ text: String(e), error: true });
    }
  };

  const files = selected ? listings[selected]?.files ?? [] : [];

  const insertFiles = useCallback(async (list: FileEntry[], position: "End" | "Next") => {
    if (list.length === 0) return;
    try {
      await invoke("playlist_insert", {
        tracks: list.map((file) => ({
          path: file.path, title: fileTitle(file), artist: fileArtist(file), durationMs: fileDuration(file),
        })),
        position,
      });
      window.dispatchEvent(new Event("playlist-changed"));
      setNotice(null);
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  }, []);

  const selectedFileList = () => files.filter((file) => selectedFiles.has(file.path));
  const selectedIndexes = () => files.map((file, i) => (selectedFiles.has(file.path) ? i : -1)).filter((i) => i >= 0);

  const selectOnly = (path: string) => {
    setSelectedFiles(new Set([path]));
    setAnchor(path);
  };

  const selectRange = (from: number, to: number) => {
    const next = new Set<string>();
    for (let i = Math.min(from, to); i <= Math.max(from, to); i++) next.add(files[i].path);
    setSelectedFiles(next);
  };

  const onFileClick = (e: React.MouseEvent, i: number) => {
    const path = files[i].path;
    if (e.shiftKey && anchor !== null) {
      const a = files.findIndex((file) => file.path === anchor);
      if (a >= 0) { selectRange(a, i); return; }
    }
    if (e.ctrlKey || e.metaKey) {
      setSelectedFiles((prev) => {
        const next = new Set(prev);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
      setAnchor(path);
      return;
    }
    selectOnly(path);
  };

  const onFileContext = (e: React.MouseEvent, i: number) => {
    e.preventDefault();
    const path = files[i].path;
    if (!selectedFiles.has(path)) selectOnly(path);
    setCtx({ x: e.clientX, y: e.clientY });
  };

  const moveSelection = (dir: 1 | -1, extend: boolean) => {
    if (files.length === 0) return;
    const idxs = selectedIndexes();
    const current = idxs.length > 0 ? idxs[idxs.length - 1] : -1;
    const next = Math.max(0, Math.min(files.length - 1, current + dir));
    if (!extend) {
      selectOnly(files[next].path);
      return;
    }
    const a = anchor !== null ? files.findIndex((file) => file.path === anchor) : -1;
    const from = a >= 0 ? a : current >= 0 ? current : next;
    selectRange(from, next);
    setAnchor(files[from].path);
  };

  const onFilesKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) return;
    const idxs = selectedIndexes();
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      moveSelection(e.key === "ArrowDown" ? 1 : -1, e.shiftKey);
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      setSelectedFiles(new Set(files.map((file) => file.path)));
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      setSelectedFiles(new Set());
      setAnchor(null);
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (idxs.length > 0) void insertFiles(selectedFileList(), "End");
    }
  };

  const prelisten = async (file: FileEntry) => {
    if (!file.track) return;
    try {
      await preview.play(file.track.id, 0);
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  };

  const copyPaths = async (list: FileEntry[]) => {
    if (list.length === 0) return;
    try {
      await navigator.clipboard.writeText(list.map((file) => file.path).join("\n"));
      setNotice({ text: t("contextMenu.pathCopied") });
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  };

  const fmt = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
  };

  const menuFiles = selectedFileList();
  const menuFile = menuFiles.length === 1 ? menuFiles[0] : null;
  const menuTitle = menuFile ? fileTitle(menuFile) : t("contextMenu.selected", { count: selectedFiles.size });

  const fileMenu = (): ContextEntry[] => [
    {
      label: t("contextMenu.addToPlaylist"), disabled: menuFiles.length === 0,
      action: () => { void insertFiles(menuFiles, "End"); },
    },
    {
      label: t("contextMenu.playNext"), disabled: menuFiles.length === 0,
      action: () => { void insertFiles(menuFiles, "Next"); },
    },
    {
      label: t("contextMenu.prelisten"), disabled: menuFiles.length !== 1 || !menuFile?.track,
      action: () => { if (menuFile) void prelisten(menuFile); },
    },
    "separator",
    {
      label: t("contextMenu.openFolder"), disabled: menuFile === null,
      action: () => {
        if (menuFile) invoke("reveal_in_folder", { path: menuFile.path }).catch((e) => setNotice({ text: String(e), error: true }));
      },
    },
    {
      label: t("contextMenu.copyPath"), disabled: menuFiles.length === 0,
      action: () => { void copyPaths(menuFiles); },
    },
  ];

  const renderNode = (entry: DirEntry, level: number): React.ReactNode => {
    const path = entry.path;
    const isExpanded = expanded.has(path);
    const listing = listings[path];
    const ancestor = containingFolder(path);
    const checked = isLibraryFolder(path) || ancestor !== null;
    const indent = 8 + level * 14;
    return (
      <Fragment key={path}>
        <div style={S.node} data-testid="folder-node" data-path={path}>
          <span style={{ width: level * 14, flexShrink: 0 }} />
          <button type="button" style={S.expander} onClick={() => toggleExpand(path)}>
            {isExpanded ? "▾" : "▸"}
          </button>
          <input type="checkbox" style={S.checkbox}
            checked={checked} disabled={ancestor !== null}
            title={ancestor !== null ? t("library.folders.includedBy", { folder: ancestor }) : t("library.folders.include")}
            onChange={() => { void onCheck(path); }}
            onClick={(e) => e.stopPropagation()} />
          <span style={{ ...S.nodeName, ...(selected === path ? S.nodeSelected : {}) }}
            title={path} onClick={() => selectFolder(path)}>
            {entry.name}
          </span>
        </div>
        {isExpanded && loading.has(path) && !listing && (
          <div style={{ ...S.nodeInfo, paddingLeft: indent + 18 }}>{t("library.folders.loading")}</div>
        )}
        {errors[path] && (
          <div style={{ ...S.nodeError, paddingLeft: indent + 18 }}>
            {t("library.folders.readError", { error: errors[path] })}
          </div>
        )}
        {isExpanded && listing && listing.dirs.map((child) => renderNode(child, level + 1))}
      </Fragment>
    );
  };

  return (
    <div style={S.wrap}>
      <div style={S.header}>
        <h3 style={S.title}>{t("library.folders.libraryFolders")}</h3>
        <div style={S.headerButtons}>
          <button style={S.btn} onClick={() => { void addFolders(); }}>{t("library.folders.add")}</button>
          <button style={S.btn} disabled={busy} onClick={() => { void rescan(); }}>{t("library.folders.rescan")}</button>
        </div>
      </div>

      {notice && (
        <div style={{ ...S.notice, color: notice.error ? "var(--red)" : "var(--text-secondary)" }}>
          {notice.text}
        </div>
      )}

      <div style={S.folderList}>
        {folders.length === 0 && <div style={S.muted}>{t("library.folders.none")}</div>}
        {folders.map((folder) => (
          <div key={folder.path} style={S.folderRow} data-testid="library-folder-row">
            <span style={S.folderPath} title={folder.path}>
              <span style={S.folderName}>{baseName(folder.path)}</span>
              <span style={S.folderParent}>{parentPath(folder.path)}</span>
            </span>
            <span style={S.folderCount}>{t("library.folders.tracks", { count: folder.track_count })}</span>
            <button type="button" style={S.removeBtn}
              aria-label={t("library.folders.removeTitle", { folder: folder.path })}
              title={t("library.folders.removeTitle", { folder: folder.path })}
              onClick={() => setRemoveTarget(folder.path)}>
              ✕
            </button>
          </div>
        ))}
      </div>

      <div style={S.treeSection}>
        <div style={S.treeHeading}>{t("library.folders.thisComputer")}</div>
        <div style={S.tree}>
          {roots.map((root) => renderNode(root, 0))}
        </div>
      </div>

      <div ref={filesRef} style={S.files} tabIndex={0} onKeyDown={onFilesKeyDown}
        onClick={() => filesRef.current?.focus()}>
        {!selected && <div style={S.empty}>{t("library.folders.pick")}</div>}
        {selected && !listings[selected] && (
          <div style={S.empty}>
            {errors[selected]
              ? t("library.folders.readError", { error: errors[selected] })
              : t("library.folders.loading")}
          </div>
        )}
        {selected && listings[selected] && files.length === 0 && (
          <div style={S.empty}>{t("library.folders.empty")}</div>
        )}
        {files.map((file, i) => (
          <div key={file.path} data-testid="folder-file" data-row-index={i} draggable
            style={{ ...S.fileRow, ...(selectedFiles.has(file.path) ? S.selected : {}), ...(file.track ? {} : S.dimmed) }}
            title={file.path}
            onClick={(e) => onFileClick(e, i)}
            onContextMenu={(e) => onFileContext(e, i)}
            onDoubleClick={() => { void insertFiles([file], "End"); }}
            onDragStart={(e) => {
              e.dataTransfer.effectAllowed = "copy";
              e.dataTransfer.setData("application/x-riars-track", JSON.stringify({
                id: file.track?.id ?? null, title: fileTitle(file), file_path: file.path, duration_ms: fileDuration(file),
              }));
            }}>
            <div style={S.fileBody}>
              <div style={S.fileName} title={file.track ? file.path : t("library.folders.notInLibrary")}>
                {file.track ? file.track.title : file.name}
              </div>
              {file.track && (
                <div style={S.meta}>
                  <span>{file.track.artist || t("library.unknownArtist")}</span>
                  {file.track.duration_ms > 0 && (
                    <span style={{ fontFamily: "var(--font-mono)" }}>{fmt(file.track.duration_ms)}</span>
                  )}
                </div>
              )}
            </div>
          </div>
        ))}
      </div>

      {ctx && (
        <ContextMenu x={ctx.x} y={ctx.y} title={menuTitle} items={fileMenu()} onClose={() => setCtx(null)} />
      )}

      {removeTarget && (
        <div style={S.overlay} onMouseDown={(e) => { if (e.target === e.currentTarget) setRemoveTarget(null); }}>
          <div role="dialog" aria-modal="true" style={S.dialog}
            aria-label={t("library.folders.removeTitle", { folder: removeTarget })}>
            <div style={S.dialogTitle}>{t("library.folders.removeTitle", { folder: removeTarget })}</div>
            <div style={S.dialogText}>{t("library.folders.removeText")}</div>
            <div style={S.dialogButtons}>
              <button autoFocus style={S.btn} onClick={() => setRemoveTarget(null)}>{t("library.folders.cancel")}</button>
              <button style={S.btn} onClick={() => { void removeFolder(false); }}>{t("library.folders.removeKeepTracks")}</button>
              <button style={S.dangerBtn} onClick={() => { void removeFolder(true); }}>{t("library.folders.removeWithTracks")}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
