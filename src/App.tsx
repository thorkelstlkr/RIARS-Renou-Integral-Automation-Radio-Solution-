import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import TransportControls from "./components/TransportControls";
import PlaylistPanel from "./components/PlaylistPanel";
import SchedulePanel from "./components/SchedulePanel";
import MediaLibrary from "./components/MediaLibrary";
import NowPlaying from "./components/NowPlaying";
import VuMeter from "./components/VuMeter";
import StreamStatus from "./components/StreamStatus";
import LanguageSelector from "./components/LanguageSelector";
import OnAirBar from "./automation/OnAirBar";
import AutomationCard from "./automation/AutomationCard";
import LogPane from "./automation/LogPane";
import StatusBar from "./automation/StatusBar";
import ClocksPanel from "./automation/ClocksPanel";
import GridPanel from "./automation/GridPanel";
import CategoriesPanel from "./automation/CategoriesPanel";
import AdsPanel from "./automation/AdsPanel";
import LogPanel from "./automation/LogPanel";
import MenuBar from "./menu/MenuBar";
import { TABS, TAB_LABEL, useAppMenus, type DialogId, type Tab } from "./menu/useAppMenus";
import PlaybackSettingsDialog from "./menu/PlaybackSettingsDialog";
import AudioSettingsDialog from "./menu/AudioSettingsDialog";
import PreviewDialog from "./menu/PreviewDialog";
import AboutDialog from "./menu/AboutDialog";
import HelpDialog from "./help/HelpDialog";

const APP_STYLES: Record<string, React.CSSProperties> = {
  app: { display: "flex", flexDirection: "column", height: "100vh", overflow: "hidden" },
  header: {
    display: "flex", alignItems: "center", gap: 16, padding: "6px 16px",
    background: "var(--bg-secondary)", borderBottom: "1px solid var(--border)", minHeight: 48,
  },
  logo: { fontSize: 16, fontWeight: 700, color: "var(--accent)", whiteSpace: "nowrap" as const, letterSpacing: -0.3 },
  logoSub: { fontSize: 10, color: "var(--text-muted)", fontWeight: 400, display: "block", letterSpacing: 0.5 },
  main: { display: "flex", flex: 1, overflow: "hidden" },
  sidebar: {
    width: 280, minWidth: 200, borderRight: "1px solid var(--border)",
    background: "var(--bg-secondary)", display: "flex", flexDirection: "column" as const,
  },
  library: { flex: 1, minHeight: 0, overflowY: "auto" as const },
  log: { height: "34%", minHeight: 120 },
  center: { flex: 1, minWidth: 0, background: "var(--bg-primary)", display: "flex", flexDirection: "column" as const },
  right: {
    width: 260, minWidth: 200, borderLeft: "1px solid var(--border)",
    overflowY: "auto" as const, background: "var(--bg-secondary)", display: "flex", flexDirection: "column" as const,
  },
  tabs: { display: "flex", gap: 2, padding: "6px 12px 0", borderBottom: "1px solid var(--border)", flexWrap: "wrap" as const },
  tab: {
    padding: "6px 12px", fontSize: 13, background: "transparent", color: "var(--text-secondary)",
    borderBottom: "2px solid transparent", borderRadius: 0,
  },
  tabActive: { color: "var(--text-primary)", borderBottom: "2px solid var(--accent)" },
  separator: { width: 1, background: "var(--border)", margin: "4px 6px" },
  notice: {
    position: "fixed", bottom: 36, left: "50%", transform: "translateX(-50%)", zIndex: 950,
    background: "var(--bg-panel)", border: "1px solid var(--border)", borderRadius: 6,
    padding: "6px 14px", fontSize: 13, boxShadow: "0 6px 20px rgba(0,0,0,0.4)",
  },
};

function TabContent({ tab }: { tab: Tab }) {
  switch (tab) {
    case "playlist": return <PlaylistPanel />;
    case "schedule": return <SchedulePanel />;
    case "clocks": return <ClocksPanel />;
    case "grid": return <GridPanel />;
    case "categories": return <CategoriesPanel />;
    case "ads": return <AdsPanel />;
    case "log": return <LogPanel />;
  }
}

export default function App() {
  const { t } = useTranslation();
  const [volume, setVolume] = useState(0.8);
  const [tab, setTab] = useState<Tab>("playlist");
  const [help, setHelp] = useState<{ open: boolean; topic?: string }>({ open: false });
  const [dialog, setDialog] = useState<DialogId | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // The volume control follows the engine: restored sessions, scheduled fades and
  // the web panel change it too.
  useEffect(() => {
    invoke<{ volume: number }>("engine_get_state").then((s) => setVolume(s.volume)).catch(() => {});
    const off = listen<{ volume: number }>("engine-state", (e) => setVolume(e.payload.volume));
    return () => { off.then((f) => f()); };
  }, []);

  const openHelp = useCallback((topic?: string) => setHelp({ open: true, topic }), []);
  const closeDialog = useCallback(() => setDialog(null), []);
  const { menus, onMenuOpen } = useAppMenus({ tab, setTab, openHelp, openDialog: setDialog, setNotice });

  // Short messages from menu actions fade after a few seconds.
  useEffect(() => {
    if (!notice) return;
    const id = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(id);
  }, [notice]);

  return (
    <div style={APP_STYLES.app}>
      <MenuBar menus={menus} onOpen={onMenuOpen} />
      <header style={APP_STYLES.header}>
        <div style={APP_STYLES.logo}>
          RIARS
          <span style={APP_STYLES.logoSub}>{t("app.subtitle")}</span>
        </div>
        <TransportControls volume={volume} onVolumeChange={setVolume} />
        <StreamStatus />
        <LanguageSelector />
      </header>
      <OnAirBar />

      <main style={APP_STYLES.main}>
        <aside style={APP_STYLES.sidebar}>
          <div style={APP_STYLES.library}><MediaLibrary /></div>
          <div style={APP_STYLES.log}><LogPane /></div>
        </aside>
        <section style={APP_STYLES.center}>
          <nav style={APP_STYLES.tabs} role="tablist">
            {TABS.map((id) => (
              <span key={id} style={{ display: "flex" }}>
                {id === "clocks" && <span style={APP_STYLES.separator} aria-hidden />}
                <button role="tab" aria-selected={tab === id} onClick={() => setTab(id)}
                  style={{ ...APP_STYLES.tab, ...(tab === id ? APP_STYLES.tabActive : {}) }}>
                  {t(TAB_LABEL[id])}
                </button>
              </span>
            ))}
          </nav>
          <div style={{ flex: 1, overflow: "hidden" }}>
            <TabContent tab={tab} />
          </div>
        </section>
        <aside style={APP_STYLES.right}>
          <NowPlaying />
          <VuMeter />
          <AutomationCard />
        </aside>
      </main>

      <StatusBar />

      {notice && <div style={APP_STYLES.notice} role="status">{notice}</div>}
      {(dialog === "crossfade" || dialog === "deadAir" || dialog === "normalization") && (
        <PlaybackSettingsDialog focus={dialog} onClose={closeDialog} />
      )}
      {dialog === "audio" && <AudioSettingsDialog onClose={closeDialog} />}
      {dialog === "preview" && <PreviewDialog onClose={closeDialog} />}
      {dialog === "about" && <AboutDialog onClose={closeDialog} />}
      <HelpDialog open={help.open} topic={help.topic} onClose={() => setHelp({ open: false })} />
    </div>
  );
}
