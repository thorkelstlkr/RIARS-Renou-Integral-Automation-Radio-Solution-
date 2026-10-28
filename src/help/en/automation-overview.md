---
id: automation-overview
title: Automation overview
section: automation
order: 1
keywords: automation, generator, categories, clocks, grid, ahead, warnings
---

# Automation overview

Long-term automation lets RIARS program the station for days or weeks without an operator. It has four parts that fit together:

1. **Categories** group the music and other audio the generator may choose.
2. **Format clocks** describe what one hour sounds like: songs, jingles, station IDs and ad breaks.
3. **The grid** decides which clock or saved playlist plays in each hour of the week, with exceptions by date.
4. **The generator** picks the actual items and adds them to the end of the on-air playlist.

All of this is stored in the library database. The schedule of events is separate and lives in `riars_schedule.json`; see [Scheduled events](topic:events).

## Turning it on

In the desktop app, click the **Automation** card toggle so it reads **ON**, use **Automation › Automation on/off**, or press **Ctrl+E**. In the browser panel, use the automation toggle. Turning automation on or off does not cut what is playing: the generator starts adding items behind the current one. Turning it off leaves the current playlist alone.

The card shows the **Clock** or **Playlist** being prepared, the **Generating** period (`HH:MM–HH:MM`) and the **Grid rule**. Warnings appear in yellow and in the **Log** pane.

## Generating ahead

The generator works ahead of the air, so there is always something ready to play. By default it keeps at least **3 items** and **15 minutes** of audio in front of the current item (adjustable in **Station settings**). It runs when the current track changes or when the amount ahead falls below that horizon. It does not rewrite items already in the playlist; changes to categories, clocks, the grid or campaigns apply from the next generated item.

If a category is empty it uses the **Fallback category**. If there is still nothing, it warns rather than leaving the station silent.

## The pieces

- [Categories](topic:categories): kinds, subcategories, smart filters, weights, separation.
- [Format clocks](topic:clocks): slots and anchors.
- [The grid](topic:grid): days, hours and exceptions.
- [Preview](topic:preview): what would air.
- [Station settings](topic:station-settings): defaults for the whole station.
