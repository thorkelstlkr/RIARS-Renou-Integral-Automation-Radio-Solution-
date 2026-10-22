import { useState } from "react";
import { useTranslation } from "react-i18next";

interface PlaylistItem {
  id: string;
  titleKey: string;
  artistKey: string;
  duration: string;
}

const DEMO: PlaylistItem[] = [
  { id: "1", titleKey: "playlist.demo.welcome", artistKey: "playlist.demo.jingle", duration: "0:15" },
  { id: "2", titleKey: "playlist.demo.localNews", artistKey: "playlist.demo.newsroom", duration: "5:00" },
  { id: "3", titleKey: "playlist.demo.morningMusic", artistKey: "playlist.demo.autoPlaylist", duration: "45:00" },
  { id: "4", titleKey: "playlist.demo.adBreak", artistKey: "playlist.demo.sponsor", duration: "0:30" },
];

const S: Record<string, React.CSSProperties> = {
  panel: { display: "flex", flexDirection: "column", height: "100%" },
  toolbar: {
    display: "flex", alignItems: "center", gap: 8, padding: "10px 16px",
    borderBottom: "1px solid var(--border)",
  },
  title: { fontSize: 14, fontWeight: 600, flex: 1 },
  btn: {
    padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12,
    background: "var(--bg-panel)",
  },
  hdr: {
    display: "flex", padding: "6px 16px", fontSize: 11, fontWeight: 600,
    color: "var(--text-muted)", textTransform: "uppercase", letterSpacing: 0.5,
    borderBottom: "1px solid var(--border)",
  },
  list: { flex: 1, overflowY: "auto" },
  row: {
    display: "flex", padding: "8px 16px", cursor: "pointer",
    borderBottom: "1px solid rgba(255,255,255,0.03)",
  },
  active: {
    background: "var(--bg-panel)", borderLeft: "3px solid var(--accent)",
  },
  num: { width: 36, color: "var(--text-muted)", fontFamily: "var(--font-mono)", fontSize: 12 },
  tit: { flex: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  art: { flex: 1, color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  dur: { width: 70, textAlign: "right" as const, fontFamily: "var(--font-mono)", fontSize: 12, color: "var(--text-secondary)" },
  foot: { padding: "6px 16px", fontSize: 12, color: "var(--text-muted)", borderTop: "1px solid var(--border)" },
};

export default function PlaylistPanel() {
  const { t } = useTranslation();
  const [items] = useState(DEMO);
  const [activeId, setActiveId] = useState("1");

  return (
    <div style={S.panel}>
      <div style={S.toolbar}>
        <h2 style={S.title}>{t("playlist.title")}</h2>
        <button style={S.btn}>{t("playlist.add")}</button>
        <button style={S.btn}>{t("playlist.clear")}</button>
      </div>
      <div style={S.hdr}>
        <span style={S.num}>{t("playlist.headerNumber")}</span>
        <span style={S.tit}>{t("playlist.headerTitle")}</span>
        <span style={S.art}>{t("playlist.headerType")}</span>
        <span style={S.dur}>{t("playlist.headerDuration")}</span>
      </div>
      <div style={S.list}>
        {items.map((item, i) => (
          <div key={item.id}
            style={{ ...S.row, ...(item.id === activeId ? S.active : {}) }}
            onClick={() => setActiveId(item.id)}>
            <span style={S.num}>{i + 1}</span>
            <span style={S.tit}>{t(item.titleKey)}</span>
            <span style={S.art}>{t(item.artistKey)}</span>
            <span style={S.dur}>{item.duration}</span>
          </div>
        ))}
      </div>
      <div style={S.foot}>{t("playlist.itemCount", { count: items.length })}</div>
    </div>
  );
}
