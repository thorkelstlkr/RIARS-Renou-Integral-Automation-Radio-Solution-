---
id: categories
title: Categories
section: automation
order: 2
keywords: category, subcategory, smart, filter, weight, separation, rules, codes
---

# Categories

A **Category** is a group of tracks the generator can choose from: a music style, jingles, station IDs, spots or a programme. Open the **Categories** tab to create and edit them.

## Creating a category

Click **New**, then fill in the **Category** editor:

| Field | Meaning |
|---|---|
| **Name** | What you see in clocks and rules |
| **Kind** | **Music**, **Jingle**, **Bumper**, **Station ID**, **Sweeper**, **Promo**, **Spot**, **Programme**, **Music bed** or **Other** |
| **Colour** | Used in the list and in the clock pie |
| **Parent** | Makes this a subcategory of another category |

A clock that asks for a parent also draws from its children. Click **Save** to keep the category.

## Smart categories

Tick **Smart category (filter instead of assigned tracks)** to let a filter choose the tracks at generation time. You can filter by **Genre**, **Artist**, **Folder starts with**, **Year from** and **Year to**, **Energy min (0-5)** and **Energy max (0-5)**, and **Min duration** / **Max duration** in seconds. This is useful when the library is large: you do not have to assign every track by hand.

## Tracks and weights

For a normal category, first save it, then choose a **Weight (0.01-100)** and use the track picker (**Search the library**) to add tracks. The table lists **Title**, **Artist**, **Length**, **Weight** and **Last played**; select rows and use **Remove selected** to take them out. A weight above 1 makes a track more likely to be picked; a weight below 1 makes it less likely.

## Separation

Tick **Own separation rules** to override the station defaults for this category. **Same song (minutes)**, **Same artist (minutes)** and **Same album (minutes)** set the minimum distance between repeats; **Imaging between others** sets how many other imaging items must pass before the same one can repeat; **Horizontal separation** avoids playing a track in the same hour as the previous day.

The station defaults are 240 minutes for the same song and 45 minutes for the same artist.

## Auto-assignment rules

Rules assign scanned tracks to this category automatically. Choose a **Field** (**Genre**, **Artist** or **Folder**), enter a **Pattern**, click **Add rule**, and later **Apply rules now**. Text matching ignores case and accents; for **Folder** the pattern is a prefix of the file path.

## Codes

Codes are free labels on a track, such as `lento`, `femenina`, `navidad` or `explícita`. Clock slots can **Require codes** or **Exclude codes**, for example pop without `explícita`. Codes are managed through the API or MCP today.

See [Format clocks](topic:clocks) for using categories in a clock, and [Station settings](topic:station-settings) for the defaults.
