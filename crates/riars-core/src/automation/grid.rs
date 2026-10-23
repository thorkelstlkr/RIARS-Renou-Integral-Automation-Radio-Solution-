//! Which grid rule applies at a given time.

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};

use super::model::GridRule;

/// The day a rule's daily span belongs to, if `at` falls inside it. A span that
/// crosses midnight belongs to the day it started on.
fn span_day(rule: &GridRule, at: NaiveDateTime) -> Option<NaiveDate> {
    let (t, date) = (at.time(), at.date());
    if rule.start == rule.end {
        Some(date)
    } else if rule.start < rule.end {
        (rule.start <= t && t < rule.end).then_some(date)
    } else if t >= rule.start {
        Some(date)
    } else if t < rule.end {
        date.pred_opt()
    } else {
        None
    }
}

fn days_in_month(date: NaiveDate) -> u32 {
    let (y, m) = (date.year(), date.month());
    let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) };
    next.and_then(|n| n.pred_opt()).map_or(31, |d| d.day())
}

fn day_matches(rule: &GridRule, day: NaiveDate) -> bool {
    let weekday = day.weekday().num_days_from_monday();
    let week = (day.day() - 1) / 7;
    let last_week = day.day() + 7 > days_in_month(day);
    rule.weekdays & (1 << weekday) != 0
        && rule.months & (1 << day.month0()) != 0
        && (rule.weeks_of_month & (1 << week) != 0 || (last_week && rule.weeks_of_month & (1 << 5) != 0))
        && rule.date_from.is_none_or(|from| day >= from)
        && rule.date_to.is_none_or(|to| day <= to)
}

pub fn rule_matches(rule: &GridRule, at: NaiveDateTime) -> bool {
    rule.enabled && span_day(rule, at).is_some_and(|day| day_matches(rule, day))
}

/// Roughly how much time a rule covers in a year: among rules of equal priority the
/// one covering less is the more specific (an exception beats the weekly grid).
pub fn coverage(rule: &GridRule) -> f64 {
    let span = if rule.start == rule.end {
        24.0 * 60.0
    } else {
        let minutes = |t: NaiveTime| (t - NaiveTime::MIN).num_minutes() as f64;
        (minutes(rule.end) - minutes(rule.start)).rem_euclid(24.0 * 60.0)
    };
    let weeks = (rule.weeks_of_month & 0x1f).count_ones() + (rule.weeks_of_month >> 5 & 1) as u32;
    let mut days = 365.0
        * (rule.weekdays & 0x7f).count_ones() as f64 / 7.0
        * (rule.months & 0xfff).count_ones() as f64 / 12.0
        * (weeks as f64 / 5.0).min(1.0);
    if let (Some(from), Some(to)) = (rule.date_from, rule.date_to) {
        days = days.min(((to - from).num_days() + 1).max(0) as f64);
    }
    span * days
}

/// The rule that applies at `at`: highest priority, then the most specific.
pub fn resolve(rules: &[GridRule], at: NaiveDateTime) -> Option<&GridRule> {
    rules.iter().filter(|r| rule_matches(r, at)).max_by(|a, b| {
        a.priority.cmp(&b.priority).then(coverage(b).total_cmp(&coverage(a)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::model::GridTarget;

    fn rule(id: &str, start: (u32, u32), end: (u32, u32)) -> GridRule {
        GridRule {
            id: id.into(),
            name: id.into(),
            priority: 0,
            target: GridTarget::Clock { clock_id: id.into() },
            weekdays: 0x7f,
            months: 0xfff,
            weeks_of_month: 0x3f,
            start: NaiveTime::from_hms_opt(start.0, start.1, 0).unwrap(),
            end: NaiveTime::from_hms_opt(end.0, end.1, 0).unwrap(),
            date_from: None,
            date_to: None,
            enabled: true,
        }
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap()
    }

    fn id(rules: &[GridRule], t: NaiveDateTime) -> Option<&str> {
        resolve(rules, t).map(|r| r.id.as_str())
    }

    #[test]
    fn dayparts_and_midnight() {
        let rules = vec![rule("day", (6, 0), (22, 0)), rule("night", (22, 0), (6, 0))];
        assert_eq!(id(&rules, at(2026, 10, 19, 6, 0)), Some("day"));
        assert_eq!(id(&rules, at(2026, 10, 19, 21, 59)), Some("day"));
        assert_eq!(id(&rules, at(2026, 10, 19, 23, 0)), Some("night"));
        assert_eq!(id(&rules, at(2026, 10, 20, 3, 0)), Some("night"));
    }

    #[test]
    fn night_span_belongs_to_the_day_it_started() {
        // Friday night 22-06 only: Saturday 03:00 is still Friday's night.
        let mut friday = rule("fri-night", (22, 0), (6, 0));
        friday.weekdays = 1 << 4;
        let rules = vec![friday];
        assert_eq!(id(&rules, at(2026, 10, 24, 3, 0)), Some("fri-night")); // Saturday
        assert_eq!(id(&rules, at(2026, 10, 23, 3, 0)), None); // Friday early morning
    }

    #[test]
    fn weekdays_months_and_weeks() {
        let mut weekend = rule("weekend", (0, 0), (0, 0));
        weekend.weekdays = 0b110_0000;
        let mut summer = rule("summer", (0, 0), (0, 0));
        summer.months = 0b0000_1110_0000; // June to August
        let mut last_sunday = rule("last-sunday", (10, 0), (12, 0));
        last_sunday.weekdays = 1 << 6;
        last_sunday.weeks_of_month = 1 << 5;
        let base = rule("base", (0, 0), (0, 0));
        let rules = vec![base, weekend, summer, last_sunday];
        assert_eq!(id(&rules, at(2026, 10, 21, 9, 0)), Some("base")); // Wednesday
        assert_eq!(id(&rules, at(2026, 10, 24, 9, 0)), Some("weekend"));
        assert_eq!(id(&rules, at(2026, 7, 15, 9, 0)), Some("summer"));
        assert_eq!(id(&rules, at(2026, 10, 25, 11, 0)), Some("last-sunday"));
        assert_eq!(id(&rules, at(2026, 10, 18, 11, 0)), Some("weekend")); // not the last Sunday
    }

    #[test]
    fn exceptions_by_date_and_priority() {
        let mut christmas = rule("christmas", (0, 0), (0, 0));
        christmas.date_from = NaiveDate::from_ymd_opt(2026, 12, 25);
        christmas.date_to = NaiveDate::from_ymd_opt(2026, 12, 25);
        let mut morning = rule("morning", (7, 0), (10, 0));
        morning.priority = 5;
        let rules = vec![rule("base", (0, 0), (0, 0)), christmas, morning];
        assert_eq!(id(&rules, at(2026, 12, 25, 18, 0)), Some("christmas"));
        assert_eq!(id(&rules, at(2026, 12, 25, 8, 0)), Some("morning")); // higher priority
        assert_eq!(id(&rules, at(2026, 12, 26, 18, 0)), Some("base"));
    }

    #[test]
    fn disabled_rules_do_not_apply() {
        let mut r = rule("off", (0, 0), (0, 0));
        r.enabled = false;
        assert_eq!(id(&[r], at(2026, 10, 19, 9, 0)), None);
    }
}
