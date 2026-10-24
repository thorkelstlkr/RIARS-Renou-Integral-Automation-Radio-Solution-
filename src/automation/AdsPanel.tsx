import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import { automation, fmtDuration, today } from "./api";
import type {
  Advertiser, Campaign, CsvKind, Daypart, DeliveryRow, GridRule, Clock, Sponsorship,
} from "./api";
import TrackPicker from "./TrackPicker";
import type { PickedTrack } from "./TrackPicker";
import { KindBadge } from "./CategoriesPanel";

type Tab = "advertisers" | "campaigns" | "delivery";
type SponsorMode = "none" | "clock" | "grid";

const WEEKDAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"] as const;

const blankAdvertiser = (): Advertiser => ({ id: 0, name: "", contact: "", notes: "" });

const blankCampaign = (advertiserId: number): Campaign => ({
  id: 0, advertiser_id: advertiserId, name: "", date_from: today(), date_to: today(),
  plays_per_day: 1, max_per_hour: 0, dayparts: [], priority: 0, sponsorship: null,
  enabled: true, spots: [],
});

const num = (value: string, fallback = 0) => {
  const n = Number(value);
  return Number.isFinite(n) ? n : fallback;
};

const hms = (value: string) => (value.length === 5 ? `${value}:00` : value.slice(0, 8));

const mmss = (seconds: number) => {
  const s = Math.max(0, Math.round(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

const sponsorMode = (s: Sponsorship | null): SponsorMode =>
  (!s ? "none" : "Clock" in s.target ? "clock" : "grid");

const S: Record<string, React.CSSProperties> = {
  panel: { display: "flex", flexDirection: "column", height: "100%" },
  toolbar: {
    display: "flex", alignItems: "center", gap: 8, padding: "8px 16px",
    borderBottom: "1px solid var(--border)",
  },
  title: { fontSize: 14, fontWeight: 600, flex: 1 },
  tab: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "transparent", color: "var(--text-secondary)" },
  tabActive: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--accent)", color: "#fff" },
  btn: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)" },
  body: { flex: 1, display: "flex", minHeight: 0 },
  left: {
    width: 300, minWidth: 220, borderRight: "1px solid var(--border)",
    display: "flex", flexDirection: "column", minHeight: 0,
  },
  leftToolbar: {
    display: "flex", alignItems: "center", gap: 6, padding: "6px 8px",
    borderBottom: "1px solid var(--border)",
  },
  list: { flex: 1, overflowY: "auto" },
  empty: { padding: "20px 12px", textAlign: "center", color: "var(--text-muted)", fontSize: 12 },
  row: {
    display: "flex", flexDirection: "column", gap: 2, padding: "6px 10px", cursor: "pointer",
    borderBottom: "1px solid rgba(255,255,255,0.03)", fontSize: 12,
  },
  rowActive: { background: "var(--bg-panel)", borderLeft: "3px solid var(--accent)" },
  rowTop: { display: "flex", alignItems: "center", gap: 6 },
  rowName: { flex: 1, fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  rowMeta: { fontSize: 11, color: "var(--text-muted)", fontFamily: "var(--font-mono)" },
  editor: {
    flex: 1, overflowY: "auto", padding: "12px 16px", display: "flex",
    flexDirection: "column", gap: 12, minHeight: 0,
  },
  section: { border: "1px solid var(--border)", borderRadius: "var(--radius)", padding: "10px 12px" },
  sectionTitle: {
    fontSize: 11, fontWeight: 700, textTransform: "uppercase" as const, letterSpacing: 0.5,
    color: "var(--text-muted)", marginBottom: 8, display: "flex", alignItems: "center", gap: 6,
  },
  grid: { display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))", gap: 8 },
  field: { display: "flex", flexDirection: "column", gap: 3, fontSize: 11, color: "var(--text-secondary)" },
  input: { fontSize: 13 },
  textarea: {
    fontSize: 13, minHeight: 70, resize: "vertical" as const, fontFamily: "inherit",
    background: "var(--bg-input)", border: "1px solid var(--border)",
    color: "var(--text-primary)", borderRadius: "var(--radius)", padding: "6px 10px", outline: "none",
  },
  check: { display: "flex", alignItems: "center", gap: 6, fontSize: 12 },
  hint: { fontSize: 11, color: "var(--text-muted)" },
  notice: { fontSize: 12, color: "var(--green)" },
  error: { fontSize: 12, color: "var(--red)" },
  actions: { display: "flex", alignItems: "flex-end", gap: 8, marginTop: 8 },
  daypart: {
    display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" as const,
    padding: "6px 0", borderBottom: "1px solid rgba(255,255,255,0.05)", fontSize: 12,
  },
  badge: { padding: "0 5px", borderRadius: 3, fontSize: 10, fontWeight: 700, lineHeight: "15px" },
  tableWrap: { flex: 1, overflow: "auto", minHeight: 0 },
  table: { width: "100%", borderCollapse: "collapse" as const, fontSize: 12 },
  th: {
    textAlign: "left" as const, padding: "5px 8px", fontSize: 10, fontWeight: 700,
    textTransform: "uppercase" as const, letterSpacing: 0.5, color: "var(--text-muted)",
    borderBottom: "1px solid var(--border)", position: "sticky" as const, top: 0,
    background: "var(--bg-primary)",
  },
  td: { padding: "4px 8px", borderBottom: "1px solid rgba(255,255,255,0.04)" },
  tdBold: { padding: "4px 8px", fontWeight: 600, borderTop: "1px solid var(--border)" },
  toolbarRow: {
    display: "flex", alignItems: "flex-end", gap: 8, padding: "8px 16px",
    borderBottom: "1px solid var(--border)", flexWrap: "wrap" as const,
  },
  noticeBar: { padding: "6px 16px", fontSize: 12 },
  delivery: { flex: 1, display: "flex", flexDirection: "column", minHeight: 0 },
  totals: { borderTop: "1px solid var(--border)", maxHeight: 200, overflow: "auto" },
};

export default function AdsPanel() {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("advertisers");
  const [advertisers, setAdvertisers] = useState<Advertiser[]>([]);
  const [campaigns, setCampaigns] = useState<Campaign[]>([]);
  const [clocks, setClocks] = useState<Clock[]>([]);
  const [grid, setGrid] = useState<GridRule[]>([]);
  const [advDraft, setAdvDraft] = useState<Advertiser | null>(null);
  const [campDraft, setCampDraft] = useState<Campaign | null>(null);
  const [advFilter, setAdvFilter] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [spotInfo, setSpotInfo] = useState<Record<number, { title: string; duration_ms: number }>>({});
  const [deliveryFrom, setDeliveryFrom] = useState(today());
  const [deliveryTo, setDeliveryTo] = useState(today());
  const [deliveryRows, setDeliveryRows] = useState<DeliveryRow[]>([]);
  const [logAdvertiser, setLogAdvertiser] = useState(0);

  const spotIds = campDraft ? campDraft.spots.map((s) => s.track_id).join(",") : "";

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [advs, camps, clks, gr, del] = await Promise.all([
          automation.advertisers(), automation.campaigns(), automation.clocks(),
          automation.grid(), automation.delivery(today(), today()),
        ]);
        if (!alive) return;
        setAdvertisers(advs);
        setCampaigns(camps);
        setClocks(clks);
        setGrid(gr);
        setDeliveryRows(del);
        if (advs.length > 0) setAdvDraft(advs[0]);
      } catch (e) {
        if (alive) setError(String(e));
      }
    })();
    return () => { alive = false; };
  }, []);

  useEffect(() => {
    const ids = spotIds
      .split(",")
      .filter(Boolean)
      .map(Number)
      .filter((id, index, all) => all.indexOf(id) === index && !(id in spotInfo));
    if (ids.length === 0) return;
    let alive = true;
    (async () => {
      try {
        const tracks = await automation.tracks(ids);
        if (!alive) return;
        setSpotInfo((info) => {
          const next = { ...info };
          for (const tr of tracks) next[tr.id] = { title: tr.title, duration_ms: tr.duration_ms };
          return next;
        });
      } catch (e) {
        if (alive) setError(String(e));
      }
    })();
    return () => { alive = false; };
  }, [spotIds, spotInfo]);

  const reloadAdvertisers = async () => {
    const list = await automation.advertisers();
    setAdvertisers(list);
    return list;
  };

  const reloadCampaigns = async () => {
    const list = await automation.campaigns();
    setCampaigns(list);
    return list;
  };

  const loadDelivery = useCallback(async () => {
    try {
      setDeliveryRows(await automation.delivery(deliveryFrom || undefined, deliveryTo || undefined));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [deliveryFrom, deliveryTo]);

  const selectAdvertiser = (adv: Advertiser) => {
    setAdvDraft({ ...adv });
    setError(null);
    setNotice(null);
  };

  const newAdvertiser = () => {
    setAdvDraft(blankAdvertiser());
    setError(null);
    setNotice(null);
  };

  const saveAdvertiser = async () => {
    if (!advDraft) return;
    try {
      const saved = await automation.saveAdvertiser(advDraft);
      await reloadAdvertisers();
      setAdvDraft(saved);
      setNotice(t("automation.ads.saved"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const deleteAdvertiser = async () => {
    if (!advDraft || advDraft.id === 0) return;
    if (!window.confirm(t("automation.ads.confirmDeleteAdvertiser", { name: advDraft.name }))) return;
    try {
      const deletedId = advDraft.id;
      await automation.deleteAdvertiser(deletedId);
      const list = await reloadAdvertisers();
      await reloadCampaigns();
      setAdvDraft(list[0] ?? null);
      setCampDraft((c) => (c && c.advertiser_id === deletedId ? null : c));
      if (advFilter === deletedId) setAdvFilter(0);
      setError(null);
      setNotice(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const selectCampaign = (camp: Campaign) => {
    setCampDraft(JSON.parse(JSON.stringify(camp)) as Campaign);
    setError(null);
    setNotice(null);
  };

  const newCampaign = () => {
    setCampDraft(blankCampaign(advFilter !== 0 ? advFilter : advertisers[0]?.id ?? 0));
    setError(null);
    setNotice(null);
  };

  const patchCamp = (p: Partial<Campaign>) => setCampDraft((c) => (c ? { ...c, ...p } : c));

  const patchDaypart = (index: number, p: Partial<Daypart>) =>
    setCampDraft((c) => (c
      ? { ...c, dayparts: c.dayparts.map((d, i) => (i === index ? { ...d, ...p } : d)) }
      : c));

  const addDaypart = () =>
    setCampDraft((c) => (c
      ? { ...c, dayparts: [...c.dayparts, { weekdays: 0x7f, start: "00:00:00", end: "23:59:59" }] }
      : c));

  const removeDaypart = (index: number) => {
    if (!window.confirm(t("automation.ads.confirmRemoveDaypart"))) return;
    setCampDraft((c) => (c ? { ...c, dayparts: c.dayparts.filter((_, i) => i !== index) } : c));
  };

  const patchSponsor = (p: Partial<Sponsorship>) =>
    setCampDraft((c) => (c && c.sponsorship ? { ...c, sponsorship: { ...c.sponsorship, ...p } } : c));

  const setSponsorMode = (mode: SponsorMode) => {
    if (mode === "none") patchCamp({ sponsorship: null });
    else if (mode === "clock") {
      patchCamp({
        sponsorship: {
          target: { Clock: { clock_id: clocks[0]?.id ?? "" } },
          opening: true, closing: true,
        },
      });
    } else {
      patchCamp({
        sponsorship: {
          target: { GridRule: { rule_id: grid[0]?.id ?? "" } },
          opening: true, closing: true,
        },
      });
    }
  };

  const addSpots = (tracks: PickedTrack[]) => {
    setCampDraft((c) => {
      if (!c) return c;
      const have = new Set(c.spots.map((s) => s.track_id));
      const spots = [...c.spots, ...tracks.filter((tr) => !have.has(tr.id)).map((tr) => ({ track_id: tr.id, weight: 1 }))];
      return { ...c, spots };
    });
    setSpotInfo((info) => {
      const next = { ...info };
      for (const tr of tracks) next[tr.id] = { title: tr.title, duration_ms: tr.duration_ms };
      return next;
    });
  };

  const setSpotWeight = (index: number, weight: number) =>
    setCampDraft((c) => (c
      ? { ...c, spots: c.spots.map((s, i) => (i === index ? { ...s, weight } : s)) }
      : c));

  const removeSpot = (index: number) => {
    const spot = campDraft?.spots[index];
    const title = spot ? (spotInfo[spot.track_id]?.title ?? `#${spot.track_id}`) : "";
    if (!window.confirm(t("automation.ads.confirmRemoveSpot", { title }))) return;
    setCampDraft((c) => (c ? { ...c, spots: c.spots.filter((_, i) => i !== index) } : c));
  };

  const saveCampaign = async () => {
    if (!campDraft) return;
    if (campDraft.advertiser_id === 0) {
      setError(t("automation.ads.selectAdvertiserFirst"));
      return;
    }
    try {
      const saved = await automation.saveCampaign(campDraft);
      await reloadCampaigns();
      setCampDraft(JSON.parse(JSON.stringify(saved)) as Campaign);
      setAdvFilter(saved.advertiser_id);
      setNotice(t("automation.ads.saved"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const deleteCampaign = async () => {
    if (!campDraft || campDraft.id === 0) return;
    if (!window.confirm(t("automation.ads.confirmDeleteCampaign", { name: campDraft.name }))) return;
    try {
      await automation.deleteCampaign(campDraft.id);
      await reloadCampaigns();
      setCampDraft(null);
      setError(null);
      setNotice(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const saveCsv = async (
    kind: CsvKind,
    name: string,
    opts: { from?: string; to?: string; advertiserId?: number; campaignId?: number },
  ) => {
    try {
      const path = await save({ defaultPath: name, filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!path) return;
      const bytes = await automation.exportCsv(kind, path, opts);
      setNotice(t("automation.ads.exported", { bytes }));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const visibleCampaigns = advFilter === 0
    ? campaigns
    : campaigns.filter((c) => c.advertiser_id === advFilter);

  const totals = new Map<number, DeliveryTotals>();
  for (const row of deliveryRows) {
    const total = totals.get(row.campaign_id) ?? {
      campaign_id: row.campaign_id, campaign: row.campaign, advertiser: row.advertiser,
      target: 0, aired: 0, partial: 0, missed: 0, seconds: 0,
    };
    total.target += row.target;
    total.aired += row.aired;
    total.partial += row.partial;
    total.missed += row.missed;
    total.seconds += row.seconds;
    totals.set(row.campaign_id, total);
  }

  return (
    <div style={S.panel}>
      <div style={S.toolbar}>
        <h2 style={S.title}>{t("automation.ads.title")}</h2>
        <button style={tab === "advertisers" ? S.tabActive : S.tab} onClick={() => setTab("advertisers")}>
          {t("automation.ads.tabAdvertisers")}
        </button>
        <button style={tab === "campaigns" ? S.tabActive : S.tab} onClick={() => setTab("campaigns")}>
          {t("automation.ads.tabCampaigns")}
        </button>
        <button style={tab === "delivery" ? S.tabActive : S.tab} onClick={() => setTab("delivery")}>
          {t("automation.ads.tabDelivery")}
        </button>
      </div>

      {(error || notice) && (
        <div style={{ ...S.noticeBar, color: error ? "var(--red)" : "var(--green)" }}>
          {error ?? notice}
        </div>
      )}

      {tab === "advertisers" && (
        <div style={S.body}>
          <div style={S.left}>
            <div style={S.leftToolbar}>
              <span style={{ ...S.rowName, fontSize: 12, color: "var(--text-secondary)" }}>
                {t("automation.ads.advertisers")}
              </span>
              <button style={S.btn} onClick={newAdvertiser}>{t("automation.ads.newAdvertiser")}</button>
            </div>
            <div style={S.list}>
              {advertisers.length === 0 && <div style={S.empty}>{t("automation.ads.emptyAdvertisers")}</div>}
              {advertisers.map((adv) => (
                <div
                  key={adv.id}
                  style={{ ...S.row, ...(advDraft?.id === adv.id ? S.rowActive : {}) }}
                  onClick={() => selectAdvertiser(adv)}
                >
                  <span style={S.rowName}>{adv.name}</span>
                  {adv.contact && <span style={S.rowMeta}>{adv.contact}</span>}
                </div>
              ))}
            </div>
          </div>
          <div style={S.editor}>
            {!advDraft && <div style={S.empty}>{t("automation.ads.selectAdvertiser")}</div>}
            {advDraft && (
              <div style={S.section}>
                <div style={S.sectionTitle}>{t("automation.ads.advertiserEditor")}</div>
                <div style={S.grid}>
                  <label style={S.field}>{t("automation.ads.name")}
                    <input style={S.input} value={advDraft.name}
                      onChange={(e) => setAdvDraft({ ...advDraft, name: e.target.value })} />
                  </label>
                  <label style={S.field}>{t("automation.ads.contact")}
                    <input style={S.input} value={advDraft.contact}
                      onChange={(e) => setAdvDraft({ ...advDraft, contact: e.target.value })} />
                  </label>
                </div>
                <label style={{ ...S.field, marginTop: 8 }}>{t("automation.ads.notes")}
                  <textarea style={S.textarea} value={advDraft.notes}
                    onChange={(e) => setAdvDraft({ ...advDraft, notes: e.target.value })} />
                </label>
                <div style={S.actions}>
                  <button style={S.btn} onClick={saveAdvertiser}>{t("automation.ads.save")}</button>
                  <button style={S.btn} onClick={deleteAdvertiser} disabled={advDraft.id === 0}>
                    {t("automation.ads.delete")}
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {tab === "campaigns" && (
        <div style={S.body}>
          <div style={S.left}>
            <div style={S.leftToolbar}>
              <select
                style={{ ...S.input, flex: 1, fontSize: 12 }}
                aria-label={t("automation.ads.filterAdvertiser")}
                value={advFilter}
                onChange={(e) => setAdvFilter(Number(e.target.value))}
              >
                <option value={0}>{t("automation.ads.allAdvertisers")}</option>
                {advertisers.map((adv) => <option key={adv.id} value={adv.id}>{adv.name}</option>)}
              </select>
              <button style={S.btn} onClick={newCampaign} disabled={advertisers.length === 0}>
                {t("automation.ads.newCampaign")}
              </button>
            </div>
            <div style={S.list}>
              {advertisers.length === 0 && <div style={S.empty}>{t("automation.ads.noAdvertisers")}</div>}
              {advertisers.length > 0 && visibleCampaigns.length === 0 && (
                <div style={S.empty}>{t("automation.ads.emptyCampaigns")}</div>
              )}
              {visibleCampaigns.map((camp) => {
                const adv = advertisers.find((a) => a.id === camp.advertiser_id);
                return (
                  <div
                    key={camp.id}
                    style={{ ...S.row, ...(campDraft?.id === camp.id ? S.rowActive : {}) }}
                    onClick={() => selectCampaign(camp)}
                  >
                    <span style={S.rowTop}>
                      <span style={S.rowName}>{camp.name}</span>
                      <span
                        style={{
                          ...S.badge,
                          color: camp.enabled ? "var(--green)" : "var(--text-muted)",
                          border: `1px solid ${camp.enabled ? "var(--green)" : "var(--border)"}`,
                        }}
                      >
                        {camp.enabled ? t("automation.ads.enabled") : t("automation.ads.disabled")}
                      </span>
                    </span>
                    <span style={S.rowMeta}>
                      {adv ? `${adv.name} · ` : ""}{camp.date_from} → {camp.date_to} · {camp.plays_per_day}/d
                    </span>
                  </div>
                );
              })}
            </div>
          </div>

          <div style={S.editor}>
            {!campDraft && <div style={S.empty}>{t("automation.ads.selectCampaign")}</div>}
            {campDraft && (
              <>
                <div style={S.section}>
                  <div style={S.sectionTitle}>{t("automation.ads.campaignEditor")}</div>
                  <div style={S.grid}>
                    <label style={S.field}>{t("automation.ads.advertiser")}
                      <select style={S.input} value={campDraft.advertiser_id}
                        onChange={(e) => patchCamp({ advertiser_id: Number(e.target.value) })}>
                        <option value={0}>{t("automation.ads.none")}</option>
                        {advertisers.map((adv) => <option key={adv.id} value={adv.id}>{adv.name}</option>)}
                      </select>
                    </label>
                    <label style={S.field}>{t("automation.ads.name")}
                      <input style={S.input} value={campDraft.name}
                        onChange={(e) => patchCamp({ name: e.target.value })} />
                    </label>
                    <label style={S.field}>{t("automation.ads.dateFrom")}
                      <input style={S.input} type="date" value={campDraft.date_from}
                        onChange={(e) => patchCamp({ date_from: e.target.value })} />
                    </label>
                    <label style={S.field}>{t("automation.ads.dateTo")}
                      <input style={S.input} type="date" value={campDraft.date_to}
                        onChange={(e) => patchCamp({ date_to: e.target.value })} />
                    </label>
                    <label style={S.field}>{t("automation.ads.playsPerDay")}
                      <input style={S.input} type="number" min={0} value={campDraft.plays_per_day}
                        onChange={(e) => patchCamp({ plays_per_day: num(e.target.value) })} />
                    </label>
                    <label style={S.field}>{t("automation.ads.maxPerHour")}
                      <input style={S.input} type="number" min={0} value={campDraft.max_per_hour}
                        onChange={(e) => patchCamp({ max_per_hour: num(e.target.value) })} />
                    </label>
                    <label style={S.field}>{t("automation.ads.priority")}
                      <input style={S.input} type="number" value={campDraft.priority}
                        onChange={(e) => patchCamp({ priority: num(e.target.value) })} />
                    </label>
                    <label style={{ ...S.check, marginTop: 16 }}>
                      <input type="checkbox" checked={campDraft.enabled}
                        onChange={(e) => patchCamp({ enabled: e.target.checked })} />
                      {t("automation.ads.enabled")}
                    </label>
                  </div>
                </div>

                <div style={S.section}>
                  <div style={S.sectionTitle}>{t("automation.ads.dayparts")}</div>
                  <div style={S.hint}>{t("automation.ads.daypartsAny")}</div>
                  {campDraft.dayparts.map((part, i) => (
                    <div key={i} style={S.daypart}>
                      <span style={{ color: "var(--text-muted)" }}>{t("automation.ads.weekdays")}</span>
                      {WEEKDAYS.map((day, bit) => (
                        <label key={day} style={{ ...S.check, gap: 2 }}>
                          <input
                            type="checkbox"
                            checked={(part.weekdays & (1 << bit)) !== 0}
                            onChange={() => patchDaypart(i, { weekdays: part.weekdays ^ (1 << bit) })}
                          />
                          {t(`automation.days.${day}`)}
                        </label>
                      ))}
                      <label style={{ ...S.field, flexDirection: "row", alignItems: "center", gap: 4 }}>
                        {t("automation.ads.start")}
                        <input style={{ ...S.input, width: 95 }} type="time" step={1} value={part.start}
                          onChange={(e) => patchDaypart(i, { start: hms(e.target.value) })} />
                      </label>
                      <label style={{ ...S.field, flexDirection: "row", alignItems: "center", gap: 4 }}>
                        {t("automation.ads.end")}
                        <input style={{ ...S.input, width: 95 }} type="time" step={1} value={part.end}
                          onChange={(e) => patchDaypart(i, { end: hms(e.target.value) })} />
                      </label>
                      <button style={S.btn} aria-label={t("automation.ads.removeDaypart")}
                        onClick={() => removeDaypart(i)}>&#10005;</button>
                    </div>
                  ))}
                  <div style={S.actions}>
                    <button style={S.btn} onClick={addDaypart}>{t("automation.ads.addDaypart")}</button>
                  </div>
                </div>

                <div style={S.section}>
                  <div style={S.sectionTitle}>{t("automation.ads.sponsorship")}</div>
                  <div style={S.grid}>
                    <label style={S.field}>{t("automation.ads.sponsorship")}
                      <select style={S.input} value={sponsorMode(campDraft.sponsorship)}
                        onChange={(e) => setSponsorMode(e.target.value as SponsorMode)}>
                        <option value="none">{t("automation.ads.sponsorNone")}</option>
                        <option value="clock">{t("automation.ads.sponsorClock")}</option>
                        <option value="grid">{t("automation.ads.sponsorGridRule")}</option>
                      </select>
                    </label>
                    {campDraft.sponsorship && "Clock" in campDraft.sponsorship.target && (
                      <label style={S.field}>{t("automation.ads.clock")}
                        <select
                          style={S.input}
                          value={campDraft.sponsorship.target.Clock.clock_id}
                          onChange={(e) => patchSponsor({ target: { Clock: { clock_id: e.target.value } } })}
                        >
                          {clocks.length === 0 && <option value="">{t("automation.ads.none")}</option>}
                          {clocks.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
                        </select>
                      </label>
                    )}
                    {campDraft.sponsorship && "GridRule" in campDraft.sponsorship.target && (
                      <label style={S.field}>{t("automation.ads.gridRule")}
                        <select
                          style={S.input}
                          value={campDraft.sponsorship.target.GridRule.rule_id}
                          onChange={(e) => patchSponsor({ target: { GridRule: { rule_id: e.target.value } } })}
                        >
                          {grid.length === 0 && <option value="">{t("automation.ads.none")}</option>}
                          {grid.map((r) => <option key={r.id} value={r.id}>{r.name}</option>)}
                        </select>
                      </label>
                    )}
                  </div>
                  {campDraft.sponsorship && (
                    <div style={{ display: "flex", gap: 12, marginTop: 8 }}>
                      <label style={S.check}>
                        <input type="checkbox" checked={campDraft.sponsorship.opening}
                          onChange={(e) => patchSponsor({ opening: e.target.checked })} />
                        {t("automation.ads.opening")}
                      </label>
                      <label style={S.check}>
                        <input type="checkbox" checked={campDraft.sponsorship.closing}
                          onChange={(e) => patchSponsor({ closing: e.target.checked })} />
                        {t("automation.ads.closing")}
                      </label>
                    </div>
                  )}
                </div>

                <div style={S.section}>
                  <div style={S.sectionTitle}>
                    {t("automation.ads.spots")} <KindBadge kind="Spot" />
                  </div>
                  <TrackPicker multi onPick={addSpots} />
                  {campDraft.spots.length > 0 ? (
                    <table style={{ ...S.table, marginTop: 8 }}>
                      <thead>
                        <tr>
                          <th style={S.th}>{t("automation.ads.spotTitle")}</th>
                          <th style={S.th}>{t("automation.ads.spotLength")}</th>
                          <th style={S.th}>{t("automation.ads.spotWeight")}</th>
                          <th style={S.th} />
                        </tr>
                      </thead>
                      <tbody>
                        {campDraft.spots.map((spot, i) => {
                          const info = spotInfo[spot.track_id];
                          return (
                            <tr key={spot.track_id}>
                              <td style={S.td}>{info ? info.title : `#${spot.track_id}`}</td>
                              <td style={{ ...S.td, fontFamily: "var(--font-mono)" }}>
                                {info ? fmtDuration(info.duration_ms) : "—"}
                              </td>
                              <td style={S.td}>
                                <input
                                  style={{ ...S.input, width: 80, padding: "2px 6px" }}
                                  type="number" min={0.01} step={0.1} value={spot.weight}
                                  aria-label={t("automation.ads.spotWeight")}
                                  onChange={(e) => setSpotWeight(i, num(e.target.value, 1))}
                                />
                              </td>
                              <td style={S.td}>
                                <button style={S.btn} aria-label={t("automation.ads.removeSpot")}
                                  onClick={() => removeSpot(i)}>&#10005;</button>
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  ) : (
                    <div style={{ ...S.hint, marginTop: 8 }}>{t("automation.ads.noSpots")}</div>
                  )}
                </div>

                <div style={S.actions}>
                  <button style={S.btn} onClick={saveCampaign}>{t("automation.ads.saveCampaign")}</button>
                  <button style={S.btn} onClick={deleteCampaign} disabled={campDraft.id === 0}>
                    {t("automation.ads.deleteCampaign")}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {tab === "delivery" && (
        <div style={S.delivery}>
          <div style={S.toolbarRow}>
            <label style={S.field}>{t("automation.ads.from")}
              <input style={S.input} type="date" value={deliveryFrom}
                onChange={(e) => setDeliveryFrom(e.target.value)} />
            </label>
            <label style={S.field}>{t("automation.ads.to")}
              <input style={S.input} type="date" value={deliveryTo}
                onChange={(e) => setDeliveryTo(e.target.value)} />
            </label>
            <button style={S.btn} onClick={loadDelivery}>{t("automation.ads.refresh")}</button>
            <span style={{ flex: 1 }} />
            <button
              style={S.btn}
              onClick={() => saveCsv("delivery", `riars-delivery-${today()}.csv`, {
                from: deliveryFrom || undefined, to: deliveryTo || undefined,
              })}
            >
              {t("automation.ads.exportDelivery")}
            </button>
            <label style={S.field}>{t("automation.ads.exportAdvertiser")}
              <select style={S.input} value={logAdvertiser}
                onChange={(e) => setLogAdvertiser(Number(e.target.value))}>
                <option value={0}>{t("automation.ads.allAdvertisers")}</option>
                {advertisers.map((adv) => <option key={adv.id} value={adv.id}>{adv.name}</option>)}
              </select>
            </label>
            <button
              style={S.btn}
              onClick={() => saveCsv("campaign-log", `riars-spot-log-${today()}.csv`, {
                from: deliveryFrom ? `${deliveryFrom}T00:00:00` : undefined,
                to: deliveryTo ? `${deliveryTo}T23:59:59` : undefined,
                advertiserId: logAdvertiser || undefined,
              })}
            >
              {t("automation.ads.exportSpotLog")}
            </button>
          </div>

          <div style={S.tableWrap}>
            <table style={S.table}>
              <thead>
                <tr>
                  <th style={S.th}>{t("automation.ads.date")}</th>
                  <th style={S.th}>{t("automation.ads.advertiser")}</th>
                  <th style={S.th}>{t("automation.ads.campaign")}</th>
                  <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.ads.target")}</th>
                  <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.ads.aired")}</th>
                  <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.ads.partial")}</th>
                  <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.ads.missed")}</th>
                  <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.ads.duration")}</th>
                </tr>
              </thead>
              <tbody>
                {deliveryRows.length === 0 && (
                  <tr><td colSpan={8} style={{ ...S.td, textAlign: "center", color: "var(--text-muted)" }}>
                    {t("automation.ads.emptyDelivery")}
                  </td></tr>
                )}
                {deliveryRows.map((row, i) => (
                  <tr key={`${row.date}-${row.campaign_id}-${i}`}>
                    <td style={{ ...S.td, fontFamily: "var(--font-mono)" }}>{row.date}</td>
                    <td style={S.td}>{row.advertiser}</td>
                    <td style={S.td}>{row.campaign}</td>
                    <td style={{ ...S.td, textAlign: "right" as const }}>{row.target}</td>
                    <td style={{ ...S.td, textAlign: "right" as const }}>{row.aired}</td>
                    <td style={{ ...S.td, textAlign: "right" as const }}>{row.partial}</td>
                    <td style={{
                      ...S.td, textAlign: "right" as const,
                      color: row.missed > 0 ? "var(--red)" : undefined,
                      fontWeight: row.missed > 0 ? 700 : undefined,
                    }}>
                      {row.missed}
                    </td>
                    <td style={{ ...S.td, textAlign: "right" as const, fontFamily: "var(--font-mono)" }}>
                      {mmss(row.seconds)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {totals.size > 0 && (
            <div style={S.totals}>
              <table style={S.table}>
                <thead>
                  <tr>
                    <th style={S.th}>{t("automation.ads.totals")}</th>
                    <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.ads.target")}</th>
                    <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.ads.aired")}</th>
                    <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.ads.partial")}</th>
                    <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.ads.missed")}</th>
                    <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.ads.duration")}</th>
                  </tr>
                </thead>
                <tbody>
                  {[...totals.values()].map((total) => (
                    <tr key={total.campaign_id}>
                      <td style={S.tdBold}>{total.campaign} · {total.advertiser}</td>
                      <td style={{ ...S.tdBold, textAlign: "right" as const }}>{total.target}</td>
                      <td style={{ ...S.tdBold, textAlign: "right" as const }}>{total.aired}</td>
                      <td style={{ ...S.tdBold, textAlign: "right" as const }}>{total.partial}</td>
                      <td style={{
                        ...S.tdBold, textAlign: "right" as const,
                        color: total.missed > 0 ? "var(--red)" : undefined,
                      }}>
                        {total.missed}
                      </td>
                      <td style={{ ...S.tdBold, textAlign: "right" as const, fontFamily: "var(--font-mono)" }}>
                        {mmss(total.seconds)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

interface DeliveryTotals {
  campaign_id: number;
  campaign: string;
  advertiser: string;
  target: number;
  aired: number;
  partial: number;
  missed: number;
  seconds: number;
}
