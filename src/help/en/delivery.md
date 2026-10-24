---
id: delivery
title: Delivery report
section: advertising
order: 3
keywords: delivery, report, missed spots, CSV, billing, campaign, aired, cut
---

# Delivery report

The **Delivery** sub-tab of **Advertising** shows how each campaign did. It is the report you use to check a contract and to prepare billing.

## Reading the table

Choose **From** and **To**, then click **Refresh**. The table has one row per campaign and day:

| Column | Meaning |
|---|---|
| **Date** | The day |
| **Advertiser** | Company that bought the campaign |
| **Campaign** | Campaign name |
| **Target** | Spots it should have had that day |
| **Aired** | Spots that played to the end |
| **Cut** | Spots cut short |
| **Missed** | Spots that should have played but did not |
| **Time** | Total time on air, in minutes and seconds |

The **Totals** table below sums each campaign over the chosen range. **Missed** figures are shown in red when they are above zero.

## Missed and cut spots

A **Cut** spot started but did not finish; the reason is in the [Proof of play](topic:proof-of-play) log. A **Missed** spot did not air at all. When a break is skipped because its time has passed, RIARS warns and the campaigns that lost the spot stay behind their pace, so the traffic system tries to place them in later breaks automatically. Check the report after a schedule change or a long silence.

## CSV for billing

Two exports are available in the same toolbar:

- **Export delivery CSV** writes the daily delivery rows for the chosen range.
- **Export spot log CSV** writes the individual airings for the chosen **Spot log advertiser**, or for all advertisers.

In the browser panel the same files are under **Downloads** as **Delivery (CSV)** and **Spot log (CSV)**. The CSV files are plain text with a header row and can be opened in a spreadsheet. Keep them as your billing record; they do not depend on the app being open.

> Note: the figures come from the proof-of-play log, which is written when an item leaves the air. A campaign that is still airing may not appear until the spot ends.

See [Advertisers and campaigns](topic:advertisers-campaigns) and [Sponsorships](topic:sponsorships).
