//! SQLite storage for advertising traffic: advertisers, campaigns, their spots
//! and the billing reports built from the proof-of-play log.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate, NaiveDateTime};
use rusqlite::{params, OptionalExtension};

use crate::library::db::MediaLibrary;

use super::model::{Advertiser, Campaign, CampaignSpot, DeliveryRow};

const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const DATE_FORMAT: &str = "%Y-%m-%d";

pub(crate) fn init_traffic_schema(db: &rusqlite::Connection) -> Result<(), String> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS advertisers (
            id      INTEGER PRIMARY KEY AUTOINCREMENT,
            name    TEXT NOT NULL,
            contact TEXT NOT NULL DEFAULT '',
            notes   TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS campaigns (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            advertiser_id INTEGER NOT NULL REFERENCES advertisers(id) ON DELETE CASCADE,
            data_json     TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_campaigns_advertiser ON campaigns(advertiser_id);
        CREATE TABLE IF NOT EXISTS campaign_spots (
            campaign_id INTEGER NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
            track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            weight      REAL NOT NULL DEFAULT 1.0,
            PRIMARY KEY (campaign_id, track_id)
        );",
    )
    .map_err(|e| format!("DB error: {e}"))
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format(DATETIME_FORMAT).to_string()
}

fn parse_datetime(text: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(text, DATETIME_FORMAT).map_err(|e| format!("DB error: {e}"))
}

fn format_date(value: NaiveDate) -> String {
    value.format(DATE_FORMAT).to_string()
}

fn parse_date(text: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(text, DATE_FORMAT).map_err(|e| format!("DB error: {e}"))
}

fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn weekday_bit(day: NaiveDate) -> u8 {
    1 << day.weekday().num_days_from_monday()
}

impl MediaLibrary {
    pub fn list_advertisers(&self) -> Result<Vec<Advertiser>, String> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id, name, contact, notes FROM advertisers ORDER BY name")
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Advertiser {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    contact: row.get(2)?,
                    notes: row.get(3)?,
                })
            })
            .map_err(|e| format!("DB error: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("DB error: {e}"))
    }

    pub fn save_advertiser(&mut self, advertiser: &Advertiser) -> Result<i64, String> {
        let name = advertiser.name.trim();
        if name.is_empty() {
            return Err("Advertiser name is required".to_string());
        }
        if advertiser.id == 0 {
            self.conn()
                .execute(
                    "INSERT INTO advertisers (name, contact, notes) VALUES (?1, ?2, ?3)",
                    params![name, advertiser.contact, advertiser.notes],
                )
                .map_err(|e| format!("DB error: {e}"))?;
            let id = self.conn().last_insert_rowid();
            self.bump_revision();
            Ok(id)
        } else {
            let rows = self
                .conn()
                .execute(
                    "UPDATE advertisers SET name = ?1, contact = ?2, notes = ?3 WHERE id = ?4",
                    params![name, advertiser.contact, advertiser.notes, advertiser.id],
                )
                .map_err(|e| format!("DB error: {e}"))?;
            if rows == 0 {
                return Err(format!("No advertiser {}", advertiser.id));
            }
            self.bump_revision();
            Ok(advertiser.id)
        }
    }

    pub fn delete_advertiser(&mut self, id: i64) -> Result<bool, String> {
        let rows = self
            .conn()
            .execute("DELETE FROM advertisers WHERE id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(rows > 0)
    }

    pub fn list_campaigns(&self) -> Result<Vec<Campaign>, String> {
        let mut campaigns = {
            let mut stmt = self
                .conn()
                .prepare("SELECT id, advertiser_id, data_json FROM campaigns")
                .map_err(|e| format!("DB error: {e}"))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| format!("DB error: {e}"))?;
            let mut campaigns = Vec::new();
            for row in rows {
                let (id, advertiser_id, json) = row.map_err(|e| format!("DB error: {e}"))?;
                let mut campaign: Campaign =
                    serde_json::from_str(&json).map_err(|e| format!("DB error: {e}"))?;
                campaign.id = id;
                campaign.advertiser_id = advertiser_id;
                campaign.spots = Vec::new();
                campaigns.push(campaign);
            }
            campaigns
        };

        let mut spots = self.all_campaign_spots()?;
        for campaign in &mut campaigns {
            campaign.spots = spots.remove(&campaign.id).unwrap_or_default();
        }
        campaigns.sort_by(|a, b| {
            a.date_from.cmp(&b.date_from).then_with(|| a.name.cmp(&b.name))
        });
        Ok(campaigns)
    }

    pub fn get_campaign(&self, id: i64) -> Result<Option<Campaign>, String> {
        let raw: Option<(i64, String)> = self
            .conn()
            .query_row(
                "SELECT advertiser_id, data_json FROM campaigns WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("DB error: {e}"))?;
        let Some((advertiser_id, json)) = raw else {
            return Ok(None);
        };
        let mut campaign: Campaign =
            serde_json::from_str(&json).map_err(|e| format!("DB error: {e}"))?;
        campaign.id = id;
        campaign.advertiser_id = advertiser_id;
        campaign.spots = self.campaign_spots(id)?;
        Ok(Some(campaign))
    }

    pub fn save_campaign(&mut self, campaign: &Campaign) -> Result<i64, String> {
        let name = campaign.name.trim();
        if name.is_empty() {
            return Err("Campaign name is required".to_string());
        }
        if campaign.date_to < campaign.date_from {
            return Err("Campaign end date is before its start date".to_string());
        }
        for spot in &campaign.spots {
            if spot.weight <= 0.0 {
                return Err("Spot weight must be greater than zero".to_string());
            }
        }

        let mut stored = campaign.clone();
        stored.name = name.to_string();
        stored.spots = Vec::new();
        let json = serde_json::to_string(&stored).map_err(|e| format!("DB error: {e}"))?;

        let tx = self.conn_mut().transaction().map_err(|e| format!("DB error: {e}"))?;

        let advertiser_exists = tx
            .query_row(
                "SELECT 1 FROM advertisers WHERE id = ?1",
                params![campaign.advertiser_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| format!("DB error: {e}"))?;
        if advertiser_exists.is_none() {
            return Err(format!("No advertiser {}", campaign.advertiser_id));
        }
        for spot in &campaign.spots {
            let track_exists = tx
                .query_row("SELECT 1 FROM tracks WHERE id = ?1", params![spot.track_id], |_| Ok(()))
                .optional()
                .map_err(|e| format!("DB error: {e}"))?;
            if track_exists.is_none() {
                return Err(format!("No track {}", spot.track_id));
            }
        }

        let id = if campaign.id == 0 {
            tx.execute(
                "INSERT INTO campaigns (advertiser_id, data_json) VALUES (?1, ?2)",
                params![campaign.advertiser_id, json],
            )
            .map_err(|e| format!("DB error: {e}"))?;
            tx.last_insert_rowid()
        } else {
            let rows = tx
                .execute(
                    "UPDATE campaigns SET advertiser_id = ?1, data_json = ?2 WHERE id = ?3",
                    params![campaign.advertiser_id, json, campaign.id],
                )
                .map_err(|e| format!("DB error: {e}"))?;
            if rows == 0 {
                return Err(format!("No campaign {}", campaign.id));
            }
            campaign.id
        };

        tx.execute("DELETE FROM campaign_spots WHERE campaign_id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        for spot in &campaign.spots {
            tx.execute(
                "INSERT INTO campaign_spots (campaign_id, track_id, weight) VALUES (?1, ?2, ?3)",
                params![id, spot.track_id, spot.weight as f64],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        }

        tx.commit().map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(id)
    }

    pub fn delete_campaign(&mut self, id: i64) -> Result<bool, String> {
        let rows = self
            .conn()
            .execute("DELETE FROM campaigns WHERE id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(rows > 0)
    }

    pub fn campaign_plays(
        &self,
        from: NaiveDateTime,
        to: NaiveDateTime,
    ) -> Result<Vec<(i64, NaiveDateTime)>, String> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT campaign_id, aired_at FROM play_log
                 WHERE campaign_id IS NOT NULL AND complete = 1
                   AND aired_at >= ?1 AND aired_at < ?2
                 ORDER BY aired_at, id",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map(params![format_datetime(from), format_datetime(to)], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("DB error: {e}"))?;
        rows.map(|row| {
            let (campaign_id, aired_at) = row.map_err(|e| format!("DB error: {e}"))?;
            Ok((campaign_id, parse_datetime(&aired_at)?))
        })
        .collect()
    }

    pub fn delivery_report(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<DeliveryRow>, String> {
        let advertisers: HashMap<i64, String> =
            self.list_advertisers()?.into_iter().map(|a| (a.id, a.name)).collect();
        let campaigns = self.list_campaigns()?;

        let counts = self.delivery_counts(from, to)?;
        let mut rows = Vec::new();
        for campaign in campaigns {
            let advertiser =
                advertisers.get(&campaign.advertiser_id).cloned().unwrap_or_default();
            let mut day = from;
            while day <= to {
                if day >= campaign.date_from && day <= campaign.date_to {
                    let target = if campaign.enabled
                        && (campaign.dayparts.is_empty()
                            || campaign
                                .dayparts
                                .iter()
                                .any(|part| part.weekdays & weekday_bit(day) != 0))
                    {
                        campaign.plays_per_day
                    } else {
                        0
                    };
                    let (aired, partial, seconds) = counts
                        .get(&(campaign.id, format_date(day)))
                        .copied()
                        .unwrap_or((0, 0, 0));
                    let aired = aired as u32;
                    rows.push(DeliveryRow {
                        date: day,
                        campaign_id: campaign.id,
                        campaign: campaign.name.clone(),
                        advertiser_id: campaign.advertiser_id,
                        advertiser: advertiser.clone(),
                        target,
                        aired,
                        partial: partial as u32,
                        seconds: seconds as u64,
                        missed: target.saturating_sub(aired),
                    });
                }
                day = match day.succ_opt() {
                    Some(next) => next,
                    None => break,
                };
            }
        }
        rows.sort_by(|a, b| {
            a.date
                .cmp(&b.date)
                .then_with(|| a.advertiser.cmp(&b.advertiser))
                .then_with(|| a.campaign.cmp(&b.campaign))
        });
        Ok(rows)
    }

    pub fn delivery_csv(&self, from: NaiveDate, to: NaiveDate) -> Result<String, String> {
        let mut csv =
            String::from("date,advertiser,campaign,target,aired,partial,missed,seconds\r\n");
        for row in self.delivery_report(from, to)? {
            let fields = [
                format_date(row.date),
                row.advertiser,
                row.campaign,
                row.target.to_string(),
                row.aired.to_string(),
                row.partial.to_string(),
                row.missed.to_string(),
                row.seconds.to_string(),
            ];
            csv.push_str(
                &fields.iter().map(|field| csv_field(field)).collect::<Vec<_>>().join(","),
            );
            csv.push_str("\r\n");
        }
        Ok(csv)
    }

    pub fn campaign_log_csv(
        &self,
        from: NaiveDateTime,
        to: NaiveDateTime,
        advertiser_id: Option<i64>,
        campaign_id: Option<i64>,
    ) -> Result<String, String> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT p.aired_at, p.ended_at, a.name, c.data_json, p.title, p.file_path,
                        p.played_ms, p.complete, p.reason
                 FROM play_log p
                 LEFT JOIN campaigns c ON c.id = p.campaign_id
                 LEFT JOIN advertisers a ON a.id = c.advertiser_id
                 WHERE p.campaign_id IS NOT NULL
                   AND p.aired_at >= ?1 AND p.aired_at < ?2
                   AND (?3 IS NULL OR c.advertiser_id = ?3)
                   AND (?4 IS NULL OR p.campaign_id = ?4)
                 ORDER BY p.aired_at, p.id",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map(
                params![format_datetime(from), format_datetime(to), advertiser_id, campaign_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .map_err(|e| format!("DB error: {e}"))?;

        let mut csv = String::from(
            "aired_at,ended_at,advertiser,campaign,title,file_path,played_seconds,complete,reason\r\n",
        );
        for row in rows {
            let (
                aired_at,
                ended_at,
                advertiser,
                data_json,
                title,
                file_path,
                played_ms,
                complete,
                reason,
            ) = row.map_err(|e| format!("DB error: {e}"))?;
            // A deleted campaign keeps its lines (they are what was billed), unnamed.
            let campaign = match data_json {
                Some(json) => serde_json::from_str::<Campaign>(&json).map_err(|e| format!("DB error: {e}"))?.name,
                None => String::new(),
            };
            let fields = [
                aired_at,
                ended_at,
                advertiser.unwrap_or_default(),
                campaign,
                title,
                file_path,
                (played_ms.max(0) as u64 / 1000).to_string(),
                if complete != 0 { "true".to_string() } else { "false".to_string() },
                reason,
            ];
            csv.push_str(
                &fields.iter().map(|field| csv_field(field)).collect::<Vec<_>>().join(","),
            );
            csv.push_str("\r\n");
        }
        Ok(csv)
    }

    fn campaign_spots(&self, campaign_id: i64) -> Result<Vec<CampaignSpot>, String> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT track_id, weight FROM campaign_spots
                 WHERE campaign_id = ?1 ORDER BY track_id",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map(params![campaign_id], |row| {
                Ok(CampaignSpot {
                    track_id: row.get(0)?,
                    weight: row.get::<_, f64>(1)? as f32,
                })
            })
            .map_err(|e| format!("DB error: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("DB error: {e}"))
    }

    fn all_campaign_spots(&self) -> Result<HashMap<i64, Vec<CampaignSpot>>, String> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT campaign_id, track_id, weight FROM campaign_spots
                 ORDER BY campaign_id, track_id",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    CampaignSpot {
                        track_id: row.get(1)?,
                        weight: row.get::<_, f64>(2)? as f32,
                    },
                ))
            })
            .map_err(|e| format!("DB error: {e}"))?;
        let mut spots: HashMap<i64, Vec<CampaignSpot>> = HashMap::new();
        for row in rows {
            let (campaign_id, spot) = row.map_err(|e| format!("DB error: {e}"))?;
            spots.entry(campaign_id).or_default().push(spot);
        }
        Ok(spots)
    }

    fn delivery_counts(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<HashMap<(i64, String), (i64, i64, i64)>, String> {
        let from = format_datetime(
            from.and_hms_opt(0, 0, 0)
                .ok_or_else(|| "DB error: invalid start date".to_string())?,
        );
        let end = to
            .succ_opt()
            .ok_or_else(|| "DB error: invalid end date".to_string())?;
        let end = format_datetime(
            end.and_hms_opt(0, 0, 0)
                .ok_or_else(|| "DB error: invalid end date".to_string())?,
        );
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT campaign_id, substr(aired_at, 1, 10),
                        SUM(CASE WHEN complete = 1 THEN 1 ELSE 0 END),
                        SUM(CASE WHEN complete = 0 THEN 1 ELSE 0 END),
                        SUM(CASE WHEN complete = 1 THEN played_ms / 1000 ELSE 0 END)
                 FROM play_log
                 WHERE campaign_id IS NOT NULL AND aired_at >= ?1 AND aired_at < ?2
                 GROUP BY campaign_id, substr(aired_at, 1, 10)",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map(params![from, end], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(|e| format!("DB error: {e}"))?;
        let mut counts = HashMap::new();
        for row in rows {
            let (campaign_id, day, aired, partial, seconds) =
                row.map_err(|e| format!("DB error: {e}"))?;
            parse_date(&day)?;
            counts.insert((campaign_id, day), (aired, partial, seconds));
        }
        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use chrono::{Duration, NaiveTime};

    use super::*;
    use crate::audio::engine::{EndReason, PlayRecord};
    use crate::automation::model::{Daypart, SponsorTarget, Sponsorship};
    use crate::playlist::playlist::{ItemKind, PlaylistItem};

    fn remove_db_files(path: &Path) {
        let mut wal = path.as_os_str().to_os_string();
        wal.push("-wal");
        let mut shm = path.as_os_str().to_os_string();
        shm.push("-shm");
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(PathBuf::from(wal));
        let _ = std::fs::remove_file(PathBuf::from(shm));
    }

    fn temp_db(name: &str) -> (MediaLibrary, PathBuf) {
        let path =
            std::env::temp_dir().join(format!("riars-traffic-{}-{name}.db", std::process::id()));
        remove_db_files(&path);
        let lib = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        (lib, path)
    }

    fn insert_track(lib: &MediaLibrary, title: &str, file_path: &str) -> i64 {
        lib.conn()
            .execute(
                "INSERT INTO tracks (title, artist, album, genre, duration_ms, file_path)
                 VALUES (?1, 'Artist', '', '', 10000, ?2)",
                params![title, file_path],
            )
            .unwrap();
        lib.conn().last_insert_rowid()
    }

    fn advertiser(name: &str) -> Advertiser {
        Advertiser { id: 0, name: name.to_string(), contact: String::new(), notes: String::new() }
    }

    fn campaign(
        advertiser_id: i64,
        name: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Campaign {
        Campaign {
            id: 0,
            advertiser_id,
            name: name.to_string(),
            date_from: from,
            date_to: to,
            plays_per_day: 1,
            max_per_hour: 0,
            dayparts: Vec::new(),
            priority: 0,
            sponsorship: None,
            enabled: true,
            spots: Vec::new(),
        }
    }

    fn datetime(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> NaiveDateTime {
        date(y, m, d).and_hms_opt(h, min, s).unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn time(h: u32, min: u32, s: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, min, s).unwrap()
    }

    #[test]
    fn advertiser_crud_and_empty_name() {
        let (mut lib, db_path) = temp_db("advertisers");

        let mut acme = Advertiser {
            id: 0,
            name: "Acme".to_string(),
            contact: "contact".to_string(),
            notes: "notes".to_string(),
        };
        let id = lib.save_advertiser(&acme).unwrap();
        assert!(lib.save_advertiser(&advertiser("   ")).is_err());
        assert!(lib.save_advertiser(&advertiser("")).is_err());

        acme.id = id;
        acme.name = "Acme Corp".to_string();
        assert_eq!(lib.save_advertiser(&acme).unwrap(), id);

        let listed = lib.list_advertisers().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Acme Corp");
        assert_eq!(listed[0].contact, "contact");

        let mut missing = acme.clone();
        missing.id = 9999;
        assert!(lib.save_advertiser(&missing).is_err());

        assert!(lib.delete_advertiser(id).unwrap());
        assert!(!lib.delete_advertiser(id).unwrap());
        assert!(lib.list_advertisers().unwrap().is_empty());

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn campaign_round_trip_with_dayparts_sponsorship_and_spots() {
        let (mut lib, db_path) = temp_db("campaign-roundtrip");

        let adv_id = lib.save_advertiser(&advertiser("Acme")).unwrap();
        let a = insert_track(&lib, "Spot A", "/m/a.mp3");
        let b = insert_track(&lib, "Spot B", "/m/b.mp3");

        let mut summer = campaign(adv_id, "Summer", date(2024, 6, 1), date(2024, 6, 30));
        summer.plays_per_day = 4;
        summer.max_per_hour = 1;
        summer.priority = 7;
        summer.dayparts = vec![
            Daypart { weekdays: 0x1f, start: time(6, 0, 0), end: time(12, 0, 0) },
            Daypart { weekdays: 0x40, start: time(18, 0, 0), end: time(22, 0, 0) },
        ];
        summer.sponsorship = Some(Sponsorship {
            target: SponsorTarget::GridRule { rule_id: "g1".to_string() },
            opening: true,
            closing: true,
        });
        summer.spots = vec![
            CampaignSpot { track_id: a, weight: 1.5 },
            CampaignSpot { track_id: b, weight: 2.0 },
        ];
        let id = lib.save_campaign(&summer).unwrap();

        let loaded = lib.get_campaign(id).unwrap().unwrap();
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.advertiser_id, adv_id);
        assert_eq!(loaded.name, "Summer");
        assert_eq!(loaded.date_from, date(2024, 6, 1));
        assert_eq!(loaded.date_to, date(2024, 6, 30));
        assert_eq!(loaded.plays_per_day, 4);
        assert_eq!(loaded.max_per_hour, 1);
        assert_eq!(loaded.priority, 7);
        assert_eq!(loaded.dayparts, summer.dayparts);
        assert_eq!(loaded.sponsorship, summer.sponsorship);
        assert_eq!(loaded.spots, summer.spots);

        let mut updated = loaded.clone();
        updated.name = "Summer Sale".to_string();
        updated.spots = vec![CampaignSpot { track_id: b, weight: 3.0 }];
        assert_eq!(lib.save_campaign(&updated).unwrap(), id);
        let reloaded = lib.get_campaign(id).unwrap().unwrap();
        assert_eq!(reloaded.name, "Summer Sale");
        assert_eq!(reloaded.spots, vec![CampaignSpot { track_id: b, weight: 3.0 }]);

        let spring = campaign(adv_id, "Spring", date(2024, 3, 1), date(2024, 3, 31));
        lib.save_campaign(&spring).unwrap();
        let listed = lib.list_campaigns().unwrap();
        assert_eq!(
            listed.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["Spring", "Summer Sale"]
        );

        assert!(lib.delete_campaign(id).unwrap());
        assert!(!lib.delete_campaign(id).unwrap());
        assert!(lib.get_campaign(id).unwrap().is_none());

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn campaign_save_validations() {
        let (mut lib, db_path) = temp_db("campaign-validation");

        let adv_id = lib.save_advertiser(&advertiser("Acme")).unwrap();
        let track = insert_track(&lib, "Spot", "/m/spot.mp3");

        let mut bad = campaign(adv_id, "Bad", date(2024, 1, 2), date(2024, 1, 1));
        assert!(lib.save_campaign(&bad).is_err());

        bad.date_to = date(2024, 1, 3);
        bad.advertiser_id = 9999;
        assert!(lib.save_campaign(&bad).is_err());

        bad.advertiser_id = adv_id;
        bad.spots = vec![CampaignSpot { track_id: 9999, weight: 1.0 }];
        assert!(lib.save_campaign(&bad).is_err());

        bad.spots = vec![CampaignSpot { track_id: track, weight: 0.0 }];
        assert!(lib.save_campaign(&bad).is_err());

        bad.spots.clear();
        bad.name = "   ".to_string();
        assert!(lib.save_campaign(&bad).is_err());

        let mut missing = campaign(adv_id, "Missing", date(2024, 1, 1), date(2024, 1, 2));
        missing.id = 9999;
        assert!(lib.save_campaign(&missing).is_err());

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn deleting_advertiser_cascades() {
        let (mut lib, db_path) = temp_db("cascade");

        let adv_id = lib.save_advertiser(&advertiser("Acme")).unwrap();
        let track = insert_track(&lib, "Spot", "/m/spot.mp3");
        let mut c = campaign(adv_id, "Promo", date(2024, 1, 1), date(2024, 1, 31));
        c.spots = vec![CampaignSpot { track_id: track, weight: 1.0 }];
        let campaign_id = lib.save_campaign(&c).unwrap();

        assert!(lib.delete_advertiser(adv_id).unwrap());
        assert!(lib.get_campaign(campaign_id).unwrap().is_none());
        assert!(lib.list_campaigns().unwrap().is_empty());
        let spots: i64 = lib
            .conn()
            .query_row("SELECT COUNT(*) FROM campaign_spots", [], |row| row.get(0))
            .unwrap();
        assert_eq!(spots, 0);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn campaign_plays_only_complete_spots() {
        let (mut lib, db_path) = temp_db("campaign-plays");

        let adv_id = lib.save_advertiser(&advertiser("Acme")).unwrap();
        let track = insert_track(&lib, "Spot", "/m/spot.mp3");
        let mut c = campaign(adv_id, "Promo", date(2024, 1, 1), date(2024, 1, 31));
        c.spots = vec![CampaignSpot { track_id: track, weight: 1.0 }];
        let campaign_id = lib.save_campaign(&c).unwrap();

        let started = datetime(2024, 1, 2, 10, 0, 0);
        let mut complete =
            PlaylistItem::new("/m/spot.mp3".to_string(), "Spot".to_string(), "Artist".to_string(), 10_000);
        complete.track_id = Some(track);
        complete.campaign_id = Some(campaign_id);
        complete.kind = ItemKind::Spot;
        let partial = complete.clone();
        let partial_started = datetime(2024, 1, 2, 10, 5, 0);
        lib.log_plays(&[
            PlayRecord {
                item: complete,
                started_at: started,
                ended_at: started + Duration::seconds(10),
                reason: EndReason::Finished,
            },
            PlayRecord {
                item: partial,
                started_at: partial_started,
                ended_at: partial_started + Duration::seconds(2),
                reason: EndReason::Skipped,
            },
        ])
        .unwrap();

        let mut music =
            PlaylistItem::new("/m/song.mp3".to_string(), "Song".to_string(), "Artist".to_string(), 10_000);
        music.track_id = None;
        lib.log_plays(&[PlayRecord {
            item: music,
            started_at: datetime(2024, 1, 2, 10, 10, 0),
            ended_at: datetime(2024, 1, 2, 10, 10, 10),
            reason: EndReason::Finished,
        }])
        .unwrap();

        let plays =
            lib.campaign_plays(datetime(2024, 1, 2, 0, 0, 0), datetime(2024, 1, 3, 0, 0, 0)).unwrap();
        assert_eq!(plays, vec![(campaign_id, started)]);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn delivery_report_targets_missed_and_range() {
        let (mut lib, db_path) = temp_db("delivery");

        let adv_id = lib.save_advertiser(&advertiser("Acme")).unwrap();
        let track = insert_track(&lib, "Spot", "/m/spot.mp3");
        let mut c = campaign(adv_id, "Morning", date(2024, 1, 1), date(2024, 1, 7));
        c.plays_per_day = 2;
        c.dayparts =
            vec![Daypart { weekdays: 0x01, start: time(6, 0, 0), end: time(12, 0, 0) }];
        c.spots = vec![CampaignSpot { track_id: track, weight: 1.0 }];
        let campaign_id = lib.save_campaign(&c).unwrap();

        let insert = |aired: &str, complete: i64, played_ms: i64| {
            lib.conn()
                .execute(
                    "INSERT INTO play_log
                        (aired_at, ended_at, track_id, file_path, title, artist, kind,
                         campaign_id, played_ms, reason, complete)
                     VALUES (?1, ?1, ?2, '/m/spot.mp3', 'Spot', 'Artist', 'Spot', ?3, ?4,
                             'Finished', ?5)",
                    params![aired, track, campaign_id, played_ms, complete],
                )
                .unwrap();
        };
        insert("2024-01-01 07:00:00", 1, 10_000);
        insert("2024-01-01 08:00:00", 0, 3_000);

        let rows = lib.delivery_report(date(2023, 12, 31), date(2024, 1, 8)).unwrap();
        assert_eq!(rows.len(), 7);
        assert!(rows.iter().all(|row| row.date >= date(2024, 1, 1)));
        assert!(rows.iter().all(|row| row.date <= date(2024, 1, 7)));
        assert!(rows.windows(2).all(|pair| pair[0].date <= pair[1].date));

        let monday = rows.iter().find(|row| row.date == date(2024, 1, 1)).unwrap();
        assert_eq!(monday.target, 2);
        assert_eq!(monday.aired, 1);
        assert_eq!(monday.partial, 1);
        assert_eq!(monday.seconds, 10);
        assert_eq!(monday.missed, 1);

        let tuesday = rows.iter().find(|row| row.date == date(2024, 1, 2)).unwrap();
        assert_eq!(tuesday.target, 0);
        assert_eq!(tuesday.aired, 0);
        assert_eq!(tuesday.missed, 0);

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn delivery_csv_quotes_commas() {
        let (mut lib, db_path) = temp_db("delivery-csv");

        let adv_id = lib.save_advertiser(&advertiser("Acme, Inc.")).unwrap();
        let c = campaign(adv_id, "Promo", date(2024, 1, 1), date(2024, 1, 1));
        lib.save_campaign(&c).unwrap();

        let csv = lib.delivery_csv(date(2024, 1, 1), date(2024, 1, 1)).unwrap();
        assert!(csv.starts_with(
            "date,advertiser,campaign,target,aired,partial,missed,seconds\r\n"
        ));
        assert!(csv.contains("\"Acme, Inc.\""));
        assert!(csv.ends_with("\r\n"));

        drop(lib);
        remove_db_files(&db_path);
    }

    #[test]
    fn campaign_log_csv_filters_by_advertiser() {
        let (mut lib, db_path) = temp_db("campaign-log-csv");

        let acme = lib.save_advertiser(&advertiser("Acme")).unwrap();
        let other = lib.save_advertiser(&advertiser("Other")).unwrap();
        let track = insert_track(&lib, "Spot", "/m/spot.mp3");

        let mut first = campaign(acme, "Acme Promo", date(2024, 1, 1), date(2024, 1, 31));
        first.spots = vec![CampaignSpot { track_id: track, weight: 1.0 }];
        let first_id = lib.save_campaign(&first).unwrap();
        let mut second = campaign(other, "Other Promo", date(2024, 1, 1), date(2024, 1, 31));
        second.spots = vec![CampaignSpot { track_id: track, weight: 1.0 }];
        let second_id = lib.save_campaign(&second).unwrap();

        for (campaign_id, minute) in [(first_id, 0u32), (second_id, 5)] {
            let mut item = PlaylistItem::new(
                "/m/spot.mp3".to_string(),
                "Spot".to_string(),
                "Artist".to_string(),
                10_000,
            );
            item.track_id = Some(track);
            item.campaign_id = Some(campaign_id);
            item.kind = ItemKind::Spot;
            let start = datetime(2024, 1, 2, 10, minute, 0);
            lib.log_plays(&[PlayRecord {
                item,
                started_at: start,
                ended_at: start + Duration::seconds(10),
                reason: EndReason::Finished,
            }])
            .unwrap();
        }

        let mut music = PlaylistItem::new(
            "/m/song.mp3".to_string(),
            "Song".to_string(),
            "Artist".to_string(),
            10_000,
        );
        music.track_id = None;
        lib.log_plays(&[PlayRecord {
            item: music,
            started_at: datetime(2024, 1, 2, 11, 0, 0),
            ended_at: datetime(2024, 1, 2, 11, 0, 10),
            reason: EndReason::Finished,
        }])
        .unwrap();

        let csv = lib
            .campaign_log_csv(
                datetime(2024, 1, 2, 0, 0, 0),
                datetime(2024, 1, 3, 0, 0, 0),
                Some(acme),
                None,
            )
            .unwrap();
        assert!(csv.starts_with(
            "aired_at,ended_at,advertiser,campaign,title,file_path,played_seconds,complete,reason\r\n"
        ));
        assert!(csv.contains(",Acme,Acme Promo,Spot,/m/spot.mp3,10,true,Finished\r\n"));
        assert!(!csv.contains("Other Promo"));
        assert!(!csv.contains("Song"));

        let csv = lib
            .campaign_log_csv(
                datetime(2024, 1, 2, 0, 0, 0),
                datetime(2024, 1, 3, 0, 0, 0),
                None,
                Some(second_id),
            )
            .unwrap();
        assert!(csv.contains(",Other,Other Promo,Spot,/m/spot.mp3,10,true,Finished\r\n"));
        assert!(!csv.contains("Acme Promo"));

        drop(lib);
        remove_db_files(&db_path);
    }
}
