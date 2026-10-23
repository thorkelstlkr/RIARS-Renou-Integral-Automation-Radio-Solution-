//! Traffic: which campaigns' spots go into an ad break, and sponsorship spots.
//! Spots are spread evenly over each campaign's dayparts: a break takes the
//! campaigns furthest behind their pace for the day first.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

use super::grid::span_day;
use super::model::{Campaign, Daypart, SponsorTarget};

/// The day is cut into slices this long to measure a campaign's pace.
const SLICE_MINUTES: i64 = 5;
/// A campaign may run this far (in spots) ahead of an even pace before it waits.
const MAX_AHEAD: f64 = 0.5;

pub fn daypart_matches(daypart: &Daypart, at: NaiveDateTime) -> bool {
    span_day(daypart.start, daypart.end, at)
        .is_some_and(|day| daypart.weekdays & (1 << day.weekday().num_days_from_monday()) != 0)
}

/// Whether `campaign` may air spots at `at`.
pub fn campaign_active(campaign: &Campaign, at: NaiveDateTime) -> bool {
    let day = at.date();
    campaign.enabled
        && campaign.date_from <= day
        && day <= campaign.date_to
        && (campaign.dayparts.is_empty() || campaign.dayparts.iter().any(|d| daypart_matches(d, at)))
}

/// Share of the campaign's airtime of the day of `at` that has started by `at`.
fn elapsed_share(campaign: &Campaign, at: NaiveDateTime) -> f64 {
    let midnight = at.date().and_time(NaiveTime::MIN);
    let (mut total, mut elapsed) = (0u32, 0u32);
    for n in 0..(24 * 60 / SLICE_MINUTES) {
        let slice = midnight + Duration::minutes(n * SLICE_MINUTES);
        if campaign_active(campaign, slice) {
            total += 1;
            if slice <= at {
                elapsed += 1;
            }
        }
    }
    if total == 0 {
        0.0
    } else {
        elapsed as f64 / total as f64
    }
}

fn hour_of(at: NaiveDateTime) -> NaiveDateTime {
    at.date().and_hms_opt(at.hour(), 0, 0).unwrap_or(at)
}

/// A spot to put on air: which campaign and which audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpotPick {
    pub campaign_id: i64,
    pub track_id: i64,
}

#[derive(Default)]
pub struct Traffic {
    campaigns: Vec<Campaign>,
    /// Day the counts below are for.
    day: Option<NaiveDate>,
    per_day: HashMap<i64, u32>,
    per_hour: HashMap<(i64, NaiveDateTime), u32>,
    /// When each spot (track) last aired or is planned to.
    spot_last: HashMap<i64, NaiveDateTime>,
}

impl Traffic {
    pub fn set_campaigns(&mut self, campaigns: Vec<Campaign>) {
        self.campaigns = campaigns;
    }

    /// The counts are not for `day` yet: call `start_day` with its plays.
    pub fn needs_day(&self, day: NaiveDate) -> bool {
        self.day != Some(day)
    }

    /// Start counting `day` from the spots already aired on it.
    pub fn start_day(&mut self, day: NaiveDate, aired: &[(i64, NaiveDateTime)]) {
        self.day = Some(day);
        self.per_day.clear();
        self.per_hour.clear();
        for &(campaign_id, at) in aired.iter().filter(|(_, at)| at.date() == day) {
            *self.per_day.entry(campaign_id).or_default() += 1;
            *self.per_hour.entry((campaign_id, hour_of(at))).or_default() += 1;
        }
    }

    /// Spots planned or aired today for a campaign.
    pub fn done_today(&self, campaign_id: i64) -> u32 {
        self.per_day.get(&campaign_id).copied().unwrap_or(0)
    }

    /// The campaign's next spot: least recently played, scaled by weight.
    fn pick_spot(&self, campaign: &Campaign, at: NaiveDateTime) -> Option<i64> {
        campaign
            .spots
            .iter()
            .filter(|s| s.weight > 0.0)
            .max_by(|a, b| {
                let score = |track_id: i64, weight: f32| {
                    let since = self.spot_last.get(&track_id).map_or(i64::MAX / 4, |last| (at - *last).num_seconds());
                    since as f64 * weight as f64
                };
                score(a.track_id, a.weight)
                    .total_cmp(&score(b.track_id, b.weight))
                    .then(b.track_id.cmp(&a.track_id))
            })
            .map(|s| s.track_id)
    }

    fn count(&mut self, pick: SpotPick, at: NaiveDateTime) {
        if self.day == Some(at.date()) {
            *self.per_day.entry(pick.campaign_id).or_default() += 1;
            *self.per_hour.entry((pick.campaign_id, hour_of(at))).or_default() += 1;
        }
        self.spot_last.insert(pick.track_id, at);
    }

    /// Spots for a break starting at `at`, at most `max_ms` long (`length_ms` gives a
    /// spot's length). One spot per advertiser; campaigns most behind their even pace
    /// go first, after priority.
    pub fn fill_break(&mut self, at: NaiveDateTime, max_ms: u64, length_ms: &dyn Fn(i64) -> u64) -> Vec<SpotPick> {
        let hour = hour_of(at);
        let mut due: Vec<(i32, f64, usize)> = self
            .campaigns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.plays_per_day > 0 && !c.spots.is_empty() && campaign_active(c, at))
            .filter_map(|(i, c)| {
                let done = self.done_today(c.id);
                let this_hour = self.per_hour.get(&(c.id, hour)).copied().unwrap_or(0);
                if done >= c.plays_per_day || (c.max_per_hour > 0 && this_hour >= c.max_per_hour) {
                    return None;
                }
                let behind = c.plays_per_day as f64 * elapsed_share(c, at) - done as f64;
                (behind > -MAX_AHEAD).then_some((c.priority, behind, i))
            })
            .collect();
        due.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.total_cmp(&a.1)));

        let mut advertisers = HashSet::new();
        let mut total = 0;
        let mut picks = Vec::new();
        for (_, _, i) in due {
            let campaign = &self.campaigns[i];
            if advertisers.contains(&campaign.advertiser_id) {
                continue;
            }
            let Some(track_id) = self.pick_spot(campaign, at) else { continue };
            let length = length_ms(track_id);
            if total + length > max_ms {
                continue; // a shorter spot of another campaign may still fit
            }
            total += length;
            advertisers.insert(campaign.advertiser_id);
            picks.push(SpotPick { campaign_id: campaign.id, track_id });
        }
        let mut t = at;
        for pick in &picks {
            self.count(*pick, t);
            t += Duration::milliseconds(length_ms(pick.track_id) as i64);
        }
        picks
    }

    /// Sponsorship spots for the opening (or closing) of a period of `clock_id`
    /// under grid rule `rule_id`.
    pub fn sponsors(&mut self, clock_id: Option<&str>, rule_id: Option<&str>, at: NaiveDateTime, opening: bool) -> Vec<SpotPick> {
        let day = at.date();
        let picks: Vec<SpotPick> = self
            .campaigns
            .iter()
            .filter(|c| c.enabled && c.date_from <= day && day <= c.date_to)
            .filter(|c| {
                c.sponsorship.as_ref().is_some_and(|s| {
                    (if opening { s.opening } else { s.closing })
                        && match &s.target {
                            SponsorTarget::Clock { clock_id: id } => clock_id == Some(id.as_str()),
                            SponsorTarget::GridRule { rule_id: id } => rule_id == Some(id.as_str()),
                        }
                })
            })
            .filter_map(|c| self.pick_spot(c, at).map(|track_id| SpotPick { campaign_id: c.id, track_id }))
            .collect();
        for pick in &picks {
            // Sponsorship mentions do not use up the campaign's spots in breaks.
            self.spot_last.insert(pick.track_id, at);
        }
        picks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::model::{CampaignSpot, Sponsorship};

    fn time(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn at(day: u32, h: u32, m: u32) -> NaiveDateTime {
        // 2026-10-19 is a Monday.
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap().and_time(time(h, m))
    }

    fn campaign(id: i64, advertiser_id: i64, plays_per_day: u32, spots: &[i64]) -> Campaign {
        Campaign {
            id,
            advertiser_id,
            name: format!("c{id}"),
            date_from: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            date_to: NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(),
            plays_per_day,
            max_per_hour: 0,
            dayparts: vec![],
            priority: 0,
            sponsorship: None,
            enabled: true,
            spots: spots.iter().map(|&track_id| CampaignSpot { track_id, weight: 1.0 }).collect(),
        }
    }

    const THIRTY_S: &dyn Fn(i64) -> u64 = &|_| 30_000;

    /// Breaks at :15 and :45 all day; the picks of each.
    fn day_of_breaks(t: &mut Traffic, day: u32, max_ms: u64) -> Vec<(NaiveDateTime, Vec<SpotPick>)> {
        t.start_day(at(day, 0, 0).date(), &[]);
        (0..24)
            .flat_map(|h| [15, 45].map(move |m| at(day, h, m)))
            .map(|when| (when, t.fill_break(when, max_ms, THIRTY_S)))
            .collect()
    }

    #[test]
    fn daypart_spans_and_weekdays() {
        const MONDAY: u8 = 1;
        let evening = Daypart { weekdays: MONDAY, start: time(18, 0), end: time(22, 0) };
        assert!(daypart_matches(&evening, at(19, 20, 0)));
        assert!(!daypart_matches(&evening, at(19, 17, 59)));
        assert!(!daypart_matches(&evening, at(19, 22, 0))); // end excluded
        assert!(!daypart_matches(&evening, at(20, 20, 0))); // Tuesday
        // Friday night 22-02 belongs to Friday: Saturday 01:00 yes, Friday 01:00 no.
        let friday_night = Daypart { weekdays: 1 << 4, start: time(22, 0), end: time(2, 0) };
        assert!(daypart_matches(&friday_night, at(24, 1, 0)));
        assert!(!daypart_matches(&friday_night, at(23, 1, 0)));
        let weekend = Daypart { weekdays: 0b110_0000, start: time(0, 0), end: time(0, 0) };
        assert!(daypart_matches(&weekend, at(25, 13, 0)));
        assert!(!daypart_matches(&weekend, at(23, 13, 0)));
    }

    #[test]
    fn spots_spread_evenly_over_the_day() {
        let mut t = Traffic::default();
        t.set_campaigns(vec![campaign(1, 1, 12, &[100])]);
        let breaks = day_of_breaks(&mut t, 19, 180_000);
        let hours: Vec<u32> = breaks.iter().filter(|(_, p)| !p.is_empty()).map(|(w, _)| w.hour()).collect();
        assert_eq!(hours.len(), 12);
        // About one every two hours, not all in the morning.
        assert!(hours.windows(2).all(|w| w[1] - w[0] >= 1 && w[1] - w[0] <= 3), "{hours:?}");
    }

    #[test]
    fn dayparts_dates_and_hour_cap() {
        let mut morning = campaign(1, 1, 6, &[100]);
        morning.dayparts = vec![Daypart { weekdays: 0b11111, start: time(7, 0), end: time(10, 0) }];
        morning.max_per_hour = 1;
        let mut t = Traffic::default();
        t.set_campaigns(vec![morning]);
        let breaks = day_of_breaks(&mut t, 19, 180_000); // Monday
        let aired: Vec<_> = breaks.iter().filter(|(_, p)| !p.is_empty()).map(|(w, _)| *w).collect();
        // Only 7-10 h, at most one an hour: 3 of the 6 wanted.
        assert_eq!(aired, vec![at(19, 7, 15), at(19, 8, 15), at(19, 9, 15)]);
        assert!(day_of_breaks(&mut t, 24, 180_000).iter().all(|(_, p)| p.is_empty())); // Saturday
        let mut t2 = Traffic::default();
        let mut ended = campaign(1, 1, 6, &[100]);
        ended.date_to = NaiveDate::from_ymd_opt(2026, 10, 18).unwrap();
        t2.set_campaigns(vec![ended]);
        assert!(day_of_breaks(&mut t2, 19, 180_000).iter().all(|(_, p)| p.is_empty()));
    }

    #[test]
    fn one_spot_per_advertiser_and_break_length() {
        let mut t = Traffic::default();
        t.set_campaigns(vec![
            campaign(1, 1, 48, &[100]),
            campaign(2, 1, 48, &[101]), // same advertiser
            campaign(3, 2, 48, &[102]),
            campaign(4, 3, 48, &[103]),
        ]);
        t.start_day(at(19, 0, 0).date(), &[]);
        let picks = t.fill_break(at(19, 12, 15), 60_000, THIRTY_S);
        assert_eq!(picks.len(), 2); // 60 s of 30 s spots
        let picks = t.fill_break(at(19, 12, 45), 180_000, THIRTY_S);
        let advertisers: HashSet<i64> = picks.iter().map(|p| [0, 1, 1, 2, 3][p.campaign_id as usize]).collect();
        assert_eq!(advertisers.len(), picks.len());
    }

    #[test]
    fn priority_first_and_aired_spots_count() {
        let mut low = campaign(1, 1, 24, &[100]);
        low.priority = 0;
        let mut high = campaign(2, 2, 24, &[101]);
        high.priority = 5;
        let mut t = Traffic::default();
        t.set_campaigns(vec![low, high]);
        // Campaign 1 already had all its spots today (e.g. before a restart).
        let aired: Vec<_> = (0..24).map(|h| (1, at(19, h / 3, 0))).collect();
        t.start_day(at(19, 0, 0).date(), &aired);
        let picks = t.fill_break(at(19, 12, 15), 30_000, THIRTY_S);
        assert_eq!(picks, vec![SpotPick { campaign_id: 2, track_id: 101 }]);
        assert_eq!(t.fill_break(at(19, 12, 45), 30_000, THIRTY_S).len(), 1);
        assert_eq!(t.done_today(1), 24);
    }

    #[test]
    fn spots_of_a_campaign_rotate() {
        let mut t = Traffic::default();
        t.set_campaigns(vec![campaign(1, 1, 48, &[100, 101, 102])]);
        let breaks = day_of_breaks(&mut t, 19, 180_000);
        let tracks: Vec<i64> = breaks.iter().flat_map(|(_, p)| p.iter().map(|p| p.track_id)).collect();
        assert!(tracks.windows(3).all(|w| w[0] != w[1] && w[0] != w[2] && w[1] != w[2]), "{tracks:?}");
    }

    #[test]
    fn sponsorship_by_clock_or_rule() {
        let mut sponsor = campaign(1, 1, 0, &[100]);
        sponsor.sponsorship = Some(Sponsorship {
            target: SponsorTarget::Clock { clock_id: "sports".into() },
            opening: true,
            closing: false,
        });
        let mut by_rule = campaign(2, 2, 0, &[101]);
        by_rule.sponsorship = Some(Sponsorship {
            target: SponsorTarget::GridRule { rule_id: "news".into() },
            opening: false,
            closing: true,
        });
        let mut t = Traffic::default();
        t.set_campaigns(vec![sponsor, by_rule]);
        assert_eq!(t.sponsors(Some("sports"), None, at(19, 9, 0), true).len(), 1);
        assert!(t.sponsors(Some("sports"), None, at(19, 9, 0), false).is_empty());
        assert!(t.sponsors(Some("pop"), None, at(19, 9, 0), true).is_empty());
        assert_eq!(t.sponsors(Some("pop"), Some("news"), at(19, 9, 0), false)[0].campaign_id, 2);
        // Sponsorships alone never go into breaks.
        t.start_day(at(19, 0, 0).date(), &[]);
        assert!(t.fill_break(at(19, 9, 15), 180_000, THIRTY_S).is_empty());
    }
}
