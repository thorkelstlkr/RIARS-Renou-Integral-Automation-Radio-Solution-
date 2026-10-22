import { useState } from "react";

interface PlaylistItem {
  id: string;
  title: string;
  artist: string;
  duration: string;
}

const DEMO: PlaylistItem[] = [
  { id: "1", title: "Bienvenida", artist: "Jingle", duration: "0:15" },
  { id: "2", title: "Noticias locales", artist: "Redaccion", duration: "5:00" },
  { id: "3", title: "Bloque musical matinal", artist: "Playlist Auto", duration: "45:00" },
  { id: "4", title: "Cuna publicitaria", artist: "Sponsor", duration: "0:30" },
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
  const [items] = useState(DEMO);
  const [activeId, setActiveId] = useState("1");

  return (
    <div style={S.panel}>
      <div style={S.toolbar}>
        <h2 style={S.title}>Playlist</h2>
        <button style={S.btn}>+ Anadir</button>
        <button style={S.btn}>Limpiar</button>
      </div>
      <div style={S.hdr}>
        <span style={S.num}>#</span>
        <span style={S.tit}>Titulo</span>
        <span style={S.art}>Tipo</span>
        <span style={S.dur}>Duracion</span>
      </div>
      <div style={S.list}>
        {items.map((item, i) => (
          <div key={item.id}
            style={{ ...S.row, ...(item.id === activeId ? S.active : {}) }}
            onClick={() => setActiveId(item.id)}>
            <span style={S.num}>{i + 1}</span>
            <span style={S.tit}>{item.title}</span>
            <span style={S.art}>{item.artist}</span>
            <span style={S.dur}>{item.duration}</span>
          </div>
        ))}
      </div>
      <div style={S.foot}>{items.length} elementos</div>
    </div>
  );
}
