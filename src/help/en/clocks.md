---
id: clocks
title: Format clocks
section: automation
order: 3
keywords: clock, hour, slot, anchor, hard, soft, max wait, fill, pie
---

# Format clocks

A **Format clock** describes what a period sounds like: one hour by default, or another **Length (min)** from 5 to 1440. It is a list of slots, some at fixed times (anchors) and some that simply follow the previous one.

## The Clocks tab

On the left is the list of clocks with their colour and length; use **New** and **Delete**. The editor has **Name**, **Colour**, **Length (min)** and **Fill category**, then the **Slots** table with **Time**, **Type**, **Details** and **Actions**. Click **Add slot** to add one, **Save** to keep the clock and **Discard** to undo unsaved changes. The pie on the right shows how the period is divided.

## Slot types

| Type | Use |
|---|---|
| **Category** | A number of tracks from a category; set **Count** and optionally **Require codes** / **Exclude codes** |
| **Imaging** | One jingle, sweeper, bumper or station ID from an imaging category |
| **Ad break** | A block filled with spots; set its **Name**, **Max seconds**, **Intro category** and **Outro category** |
| **Playlist** | A saved playlist, whole or the next **Count** items; blank means **All** |
| **Track** | A fixed file, for example a programme theme |
| **Fill** | Music from a category until the next anchor or the end of the clock |

## Anchors

Put a time in the **Time** column as `mm:ss` to make a slot an anchor: for example `15:00` for the ad break at a quarter past, or `00:00` for a station ID at the top of the hour. Leave it empty for a slot that follows the previous one.

Anchors are **soft** by default: the block starts when the song playing at that time ends, so music is never cut. **Max wait (s)** limits how late a soft anchor may run. Tick **Hard** to cut in at the exact time; use it when you must join a network news bulletin. Hard anchors more than a minute late are dropped, for example after the machine has slept.

## Fixing the length

Between anchors the generator places the unordered slots and uses **Fill** to reach the next anchor or the end. The **Fill category** on the clock is a second-level fallback when the slots run out. Use a **Fill** slot for music that must stop at the next fixed point, and the clock-level **Fill category** as a safety net.

## The hour pie

The pie is a quick visual check: each slot gets a colour, and its size follows the planned duration. Gaps or an over-full hour show up immediately.

See [The grid](topic:grid) to decide when this clock plays, and [Station imaging](topic:imaging) for the jingles and IDs inside it.
