import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import TransportControls from "./components/TransportControls";
import PlaylistPanel from "./components/PlaylistPanel";
import SchedulePanel from "./components/SchedulePanel";
import MediaLibrary from "./components/MediaLibrary";
import NowPlaying from "./components/NowPlaying";
import VuMeter from "./components/VuMeter";
import StreamStatus from "./components/StreamStatus";
import LanguageSelector from "./components/LanguageSelector";

const APP_STYLES: Record<string, React.CSSProperties> = {
  app: { display: "flex", flexDirection: "column", height: "100vh", overflow: "hidden" },
  header: {
    display: "flex", alignItems: "center", gap: 16, padding: "8px 16px",
    background: "var(--bg-secondary)", borderBottom: "1px solid var(--border)", minHeight: 56,
  },
  logo: { fontSize: 16, fontWeight: 700, color: "var(--accent)", whiteSpace: "nowrap" as const, letterSpacing: -0.3 },
  logoSub: { fontSize: 10, color: "var(--text-muted)", fontWeight: 400, display: "block", letterSpacing: 0.5 },
  main: { display: "flex", flex: 1, overflow: "hidden" },
  sidebar: {
    width: 280, minWidth: 200, borderRight: "1px solid var(--border)",
    overflowY: "auto" as const, background: "var(--bg-secondary)",
  },
  center: { flex: 1, overflowY: "auto" as const, background: "var(--bg-primary)" },
  right: {
    width: 260, minWidth: 200, borderLeft: "1px solid var(--border)",
    overflowY: "auto" as const, background: "var(--bg-secondary)", display: "flex", flexDirection: "column" as const,
  },
  tabs: { display: "flex", gap: 2, padding: "6px 12px 0", borderBottom: "1px solid var(--border)" },
  tab: {
    padding: "6px 14px", fontSize: 13, background: "transparent", color: "var(--text-secondary)",
    borderBottom: "2px solid transparent", borderRadius: 0,
  },
  tabActive: { color: "var(--text-primary)", borderBottom: "2px solid var(--accent)" },
  footer: {
    display: "flex", alignItems: "center", justifyContent: "space-between",
    padding: "4px 16px", background: "var(--bg-secondary)", borderTop: "1px solid var(--border)",
    fontSize: 12, color: "var(--text-muted)", minHeight: 28,
  },
};

export default function App() {
  const { t } = useTranslation();
  const [volume, setVolume] = useState(0.8);
  const [tab, setTab] = useState<"playlist" | "schedule">("playlist");
  const [clock, setClock] = useState(new Date().toLocaleTimeString());

  useEffect(() => {
    const id = setInterval(() => setClock(new Date().toLocaleTimeString()), 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <div style={APP_STYLES.app}>
      <header style={APP_STYLES.header}>
        <div style={APP_STYLES.logo}>
          RIARS
          <span style={APP_STYLES.logoSub}>{t("app.subtitle")}</span>
        </div>
        <TransportControls volume={volume} onVolumeChange={setVolume} />
        <StreamStatus />
        <LanguageSelector />
      </header>

      <main style={APP_STYLES.main}>
        <aside style={APP_STYLES.sidebar}>
          <MediaLibrary />
        </aside>
        <section style={{ ...APP_STYLES.center, display: "flex", flexDirection: "column" }}>
          <nav style={APP_STYLES.tabs}>
            {(["playlist", "schedule"] as const).map((id) => (
              <button key={id} onClick={() => setTab(id)}
                style={{ ...APP_STYLES.tab, ...(tab === id ? APP_STYLES.tabActive : {}) }}>
                {t(id === "playlist" ? "playlist.title" : "schedule.title")}
              </button>
            ))}
          </nav>
          <div style={{ flex: 1, overflow: "hidden" }}>
            {tab === "playlist" ? <PlaylistPanel /> : <SchedulePanel />}
          </div>
        </section>
        <aside style={APP_STYLES.right}>
          <NowPlaying />
          <VuMeter />
        </aside>
      </main>

      <footer style={APP_STYLES.footer}>
        <span style={{ fontFamily: "var(--font-mono)" }}>{clock}</span>
        <span style={{ color: "var(--green)" }}>{t("app.version")}</span>
      </footer>
    </div>
  );
}
