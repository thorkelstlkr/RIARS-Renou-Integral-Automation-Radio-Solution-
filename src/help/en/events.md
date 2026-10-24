---
id: events
title: Scheduled events
section: scheduling
order: 1
keywords: schedule, event, once, daily, weekly, interval, action
---

# Scheduled events

The **Schedule** tab (the Scheduling section of the manual) runs simple actions at fixed times. It is separate from the long-term automation: events do something once, or on a repeating pattern, while the grid decides what airs hour by hour.

## Event types

| **When** | Meaning |
|---|---|
| **Once** | The next time that time comes; the event then disables itself |
| **Every day** | The same time every day |
| **Every week** | A weekday and time |
| **Every N minutes** | A repeating interval |

Each event has a **Name**. The list shows its **When** and its **Action**, a checkbox to enable or disable it and a cross to remove it. A disabled event stays in the list but does not run.

## Actions

| **Action** | What it does |
|---|---|
| **Play track** | Plays a chosen file immediately |
| **Play playlist** | Loads and plays a saved playlist |
| **Set volume** | Sets the output level |
| **Fade volume** | Fades to a level over a number of **Seconds** |
| **Start streaming** | Starts the Icecast connection |
| **Stop streaming** | Stops it |

To add an event, fill in **Name**, **When**, the time or **Minutes**, and **Action**, then click **Add**. For **Play track**, use **Choose file…**; for **Play playlist**, choose a saved **Playlist**.

## Precision and saving

Events run with second precision. One-off events disable themselves after firing, so they do not repeat the next day. The schedule is saved as `riars_schedule.json` next to the library database in the data folder; it is loaded at startup by both the desktop app and `riars-server`. The bottom bar shows **Event scheduler: N events**.

> Tip: use a scheduled event for a daily news feed or a volume change, and the [Grid](topic:grid) for the normal music programming. For the difference between the two systems, see [Automation overview](topic:automation-overview).
