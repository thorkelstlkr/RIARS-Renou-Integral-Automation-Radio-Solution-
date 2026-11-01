import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save } from "@tauri-apps/plugin-dialog";
import { automation, today } from "../automation/api";
import { live, type PlayMode } from "../live/api";
import { runCommand, type CommandId } from "../commands";
import type { Menu, MenuEntry } from "./MenuBar";

export const TABS = ["playlist", "schedule", "clocks", "grid", "categories", "ads", "log", "live"] as const;
export type Tab = (typeof TABS)[number];
export const TAB_LABEL: Record<Tab, string> = {
  playlist: "playlist.title",
  schedule: "schedule.title",
  clocks: "automation.tabs.clocks",
  grid: "automation.tabs.grid",
  categories: "automation.tabs.categories",
  ads: "automation.tabs.ads",
  log: "automation.tabs.log",
  live: "live.title",
};

export type DialogId = "crossfade" | "deadAir" | "normalization" | "audio" | "preview" | "about" | "mic" | "rds";
const LANGS = ["en", "es", "fr", "de", "pt", "ca"] as const;

interface Options {
  tab: Tab;
  setTab: (tab: Tab) => void;
  openHelp: (topic?: string) => void;
  openDialog: (dialog: DialogId) => void;
  setNotice: (text: string | null) => void;
}

/** The menus of the window and their keyboard shortcuts. */
export function useAppMenus({ tab, setTab, openHelp, openDialog, setNotice }: Options) {
  const { t, i18n } = useTranslation();
  const [autoOn, setAutoOn] = useState(false);
  const [mode, setMode] = useState<PlayMode>("Automatic");

  const refreshAuto = useCallback(() => {
    automation.status().then((s) => setAutoOn(s.enabled)).catch(() => {});
  }, []);
  useEffect(refreshAuto, [refreshAuto]);

  const refreshMode = useCallback(() => {
    live.status().then((s) => setMode(s.mode)).catch(() => {});
  }, []);
  useEffect(refreshMode, [refreshMode]);

  const setPlayMode = useCallback(async (next: PlayMode) => {
    try {
      await live.setMode(next);
      setMode(next);
    } catch (e) { setNotice(String(e)); }
  }, [setNotice]);

  const toggleMic = useCallback(async () => {
    try {
      const status = await live.status();
      await live.setMic(!status.mic.on);
    } catch (e) { setNotice(String(e)); }
  }, [setNotice]);

  // Panel commands need their panel on screen: show it, then send the command.
  const onTab = useCallback((target: Tab, id: CommandId) => {
    setTab(target);
    setTimeout(() => runCommand(id), 0);
  }, [setTab]);

  const toggleAutomation = useCallback(async () => {
    try {
      const status = await automation.status();
      await automation.setEnabled(!status.enabled);
      setAutoOn(!status.enabled);
      setNotice(!status.enabled ? t("automation.status.autoOn") : t("automation.status.autoOff"));
    } catch (e) { setNotice(String(e)); }
  }, [setNotice, t]);

  const exportLog = useCallback(async () => {
    try {
      const path = await save({ defaultPath: `riars-log-${today()}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!path) return;
      const bytes = await automation.exportCsv("log", path);
      setNotice(t("automation.log.exported", { bytes }));
    } catch (e) { setNotice(String(e)); }
  }, [setNotice, t]);

  const setLanguage = useCallback((lang: string) => {
    i18n.changeLanguage(lang);
    invoke("set_locale", { lang }).catch(() => {});
  }, [i18n]);

  const quit = useCallback(() => { getCurrentWindow().close().catch(() => {}); }, []);
  const call = (cmd: string) => () => { invoke(cmd).catch((e) => setNotice(String(e))); };

  const menus: Menu[] = useMemo(() => {
    const tabItems: MenuEntry[] = TABS.map((id, i) => ({
      label: t(TAB_LABEL[id]), shortcut: `Ctrl+${i + 1}`, checked: tab === id, action: () => setTab(id),
    }));
    return [
      { id: "file", label: t("menu.file"), items: [
        { label: t("menu.addFolder"), shortcut: "Ctrl+Shift+A", action: () => runCommand("library.addFolder") },
        "separator",
        { label: t("menu.openPlaylist"), shortcut: "Ctrl+O", action: () => onTab("playlist", "playlist.open") },
        { label: t("menu.savePlaylist"), shortcut: "Ctrl+S", action: () => onTab("playlist", "playlist.save") },
        "separator",
        { label: t("menu.exportLog"), action: exportLog },
        "separator",
        { label: t("menu.quit"), shortcut: "Ctrl+Q", action: quit },
      ] },
      { id: "edit", label: t("menu.edit"), items: [
        { label: t("menu.searchLibrary"), shortcut: "Ctrl+L", action: () => runCommand("library.search") },
        { label: t("menu.completeMetadata"), action: () => runCommand("library.completeMetadata") },
        { label: t("menu.clearPlaylist"), action: () => onTab("playlist", "playlist.clear") },
      ] },
      { id: "view", label: t("menu.view"), items: [
        ...tabItems,
        "separator",
        { label: t("menu.language"), submenu: LANGS.map((lng) => ({
          label: t(`language.${lng}`), checked: i18n.language === lng, action: () => setLanguage(lng),
        })) },
      ] },
      { id: "playback", label: t("menu.playback"), items: [
        { label: t("transport.play"), shortcut: "F5", action: call("engine_play") },
        { label: t("transport.pause"), shortcut: "F6", action: call("engine_pause") },
        { label: t("transport.stop"), shortcut: "F7", action: call("engine_stop") },
        { label: t("transport.next"), shortcut: "F8", action: call("engine_next") },
        "separator",
        { label: t("menu.crossfade"), action: () => openDialog("crossfade") },
        { label: t("menu.deadAir"), action: () => openDialog("deadAir") },
        { label: t("menu.normalization"), action: () => openDialog("normalization") },
        "separator",
        { label: t("menu.soundCards"), action: () => openDialog("audio") },
        { label: t("menu.microphone"), action: () => openDialog("mic") },
      ] },
      { id: "automation", label: t("menu.automation"), items: [
        { label: t("menu.automationOn"), shortcut: "Ctrl+E", checked: autoOn, action: toggleAutomation },
        { label: t("menu.preview"), action: () => openDialog("preview") },
        "separator",
        { label: t("live.mode.Automatic"), checked: mode === "Automatic", action: () => { void setPlayMode("Automatic"); } },
        { label: t("live.mode.Assisted"), checked: mode === "Assisted", action: () => { void setPlayMode("Assisted"); } },
        { label: t("live.mode.Manual"), checked: mode === "Manual", action: () => { void setPlayMode("Manual"); } },
        "separator",
        { label: t("automation.tabs.clocks"), action: () => setTab("clocks") },
        { label: t("automation.tabs.grid"), action: () => setTab("grid") },
        { label: t("automation.tabs.categories"), action: () => setTab("categories") },
        { label: t("automation.tabs.ads"), action: () => setTab("ads") },
        { label: t("automation.tabs.log"), action: () => setTab("log") },
        "separator",
        { label: t("menu.stationSettings"), action: () => onTab("categories", "categories.settings") },
      ] },
      { id: "broadcast", label: t("menu.broadcast"), items: [
        { label: t("menu.streamSettings"), action: () => runCommand("stream.settings") },
        { label: t("menu.streamStart"), action: () => runCommand("stream.start") },
        { label: t("menu.streamStop"), action: () => runCommand("stream.stop") },
        "separator",
        { label: t("menu.rds"), action: () => openDialog("rds") },
      ] },
      { id: "help", label: t("menu.help"), items: [
        { label: t("menu.manual"), shortcut: "F1", action: () => openHelp() },
        { label: t("menu.shortcuts"), action: () => openHelp("shortcuts") },
        { label: t("menu.webPanelHelp"), action: () => openHelp("web-panel") },
        "separator",
        { label: t("menu.about"), action: () => openDialog("about") },
      ] },
    ];
    // `call` is rebuilt every render but only closes over setNotice.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [t, i18n.language, tab, autoOn, mode, setTab, onTab, exportLog, quit, setLanguage, openDialog, toggleAutomation, setPlayMode, openHelp]);

  // Global shortcuts. F-keys and Ctrl combinations work even inside text fields.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // The microphone toggle (F9) also works with a dialog open.
      if (document.querySelector("[role=dialog]") && e.key !== "F1" && e.key !== "F9") return;
      const ctrl = e.ctrlKey || e.metaKey;
      const action: (() => void) | undefined = (() => {
        if (!ctrl && !e.altKey) {
          switch (e.key) {
            case "F1": return () => openHelp();
            case "F5": return call("engine_play");
            case "F6": return call("engine_pause");
            case "F7": return call("engine_stop");
            case "F8": return call("engine_next");
            case "F9": return () => { void toggleMic(); };
          }
          return undefined;
        }
        if (!ctrl) return undefined;
        const key = e.key.toLowerCase();
        if (e.shiftKey && key === "a") return () => runCommand("library.addFolder");
        if (e.shiftKey) return undefined;
        if (key >= "1" && key <= "8") return () => setTab(TABS[Number(key) - 1]);
        switch (key) {
          case "o": return () => onTab("playlist", "playlist.open");
          case "s": return () => onTab("playlist", "playlist.save");
          case "l": return () => runCommand("library.search");
          case "e": return toggleAutomation;
          case "q": return quit;
        }
        return undefined;
      })();
      if (action) {
        e.preventDefault();
        action();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openHelp, setTab, onTab, toggleAutomation, toggleMic, quit]);

  return {
    menus,
    onMenuOpen: (id: string) => {
      if (id === "automation") { refreshAuto(); refreshMode(); }
    },
  };
}
