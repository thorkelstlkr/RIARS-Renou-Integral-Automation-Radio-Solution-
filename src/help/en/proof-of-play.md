---
id: proof-of-play
title: Proof of play
section: logs
order: 1
keywords: proof of play, log, aired, complete, skipped, silence, CSV, export
---

# Proof of play

The proof-of-play log records what really aired, with the real time. An entry is written when an item **leaves the air**, not when it is planned, so a planned item that never played does not appear as aired.

## The Log tab

Open the **Log** tab to see a table with:

| Column | Meaning |
|---|---|
| **Aired** | Date and time it started |
| **Kind** | Music, jingle, spot, programme, etc. |
| **Title** and **Artist** | What played |
| **Played** | How long it was on air |
| **Complete** | A tick, or a cross with the reason |
| **Campaign** | Campaign number for a spot |

Use **From** and **To**, **Kind** and **Title or artist** to filter, then **Refresh**. The footer shows how many entries match. The table shows at most 1,000 entries for the chosen range, in time order; narrow the range to see a longer period without the limit.

An item is **Complete** when it played to its end, or to at least 95 % of its length. Otherwise the reason is one of **Finished**, **Skipped**, **Silence** (cut by the dead-air detector) or **Stopped**.

## The Log pane

The **Log** pane under the library shows the latest entries in a compact list, coloured by kind, together with automation warnings in red. It is useful during a live show to see what just happened.

## Export and retention

Click **Export CSV** to write the filtered range to a file. In the browser panel, use **Play log (CSV)** under **Downloads**. The CSV has the same fields and is suitable for billing, music reporting and archives.

The log lives in the library database. RIARS does not delete old entries automatically today, so the database grows slowly; export what you need for each period and keep the CSV as the permanent record. The design target is to keep about 13 months for annual billing.

> Tip: do not edit the library database file directly. Use the app or the API, and export before maintenance.

See [Delivery report](topic:delivery) and [Troubleshooting](topic:troubleshooting).
