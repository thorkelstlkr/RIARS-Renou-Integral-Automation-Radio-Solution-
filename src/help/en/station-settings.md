---
id: station-settings
title: Station settings
section: automation
order: 6
keywords: settings, default clock, fallback, separation, ahead, ad break, no overlap
---

# Station settings

Station settings apply to the whole automation. In the desktop app they are at the bottom of the **Categories** tab: open the **Station settings** section. The same dialog is available from **Automation › Station settings**. Click **Save settings** when you are done.

## Main settings

| Field | Meaning |
|---|---|
| **Automation enabled** | Same switch as the **Automation** card |
| **Default clock** | The clock used for hours no grid rule covers |
| **Fallback category** | Music used when a category has nothing playable |
| **Items ahead** | Minimum number of items the generator keeps in front of the air |
| **Minutes ahead** | Minimum amount of audio it keeps in front |
| **Keep played items** | How many finished items stay in the on-air playlist |
| **Ad break seconds** | Default length of an ad break when the slot does not set one |

The defaults are **3** items and **15** minutes ahead, **20** played items kept and **180** seconds for an ad break. A larger horizon gives more warning of what comes next but corrects changes later.

## Default separation

These values apply to categories that do not have **Own separation rules**:

| Field | Default |
|---|---|
| **Same song (minutes)** | 240 |
| **Same artist (minutes)** | 45 |
| **Same album (minutes)** | 0 |
| **Imaging between others** | 3 |
| **Horizontal separation** | Off |

Zero disables a separation rule. If strict rules leave a small category with no candidate, the generator relaxes them rather than leaving silence, and writes a warning.

## Imaging

**Jingle category**, **Jingle every N songs (0 = never)** and **Sweeper category on style change** control the automatic imaging. See [Station imaging](topic:imaging).

## Kinds that never crossfade

This list decides which item kinds are protected from the crossfade. By default only **Spot** is in it, so commercials start and end cleanly. Tick other kinds, such as **Station ID** or **Jingle**, if they must not overlap music.

> Note: changes apply from the next item the generator builds. Items already in the playlist keep the settings they were generated with.

See [Categories](topic:categories) and [Automation overview](topic:automation-overview).
