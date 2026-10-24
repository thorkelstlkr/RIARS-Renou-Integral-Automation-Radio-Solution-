---
id: imaging
title: Station imaging
section: automation
order: 5
keywords: imaging, jingle, sweeper, bumper, station ID, intro, outro, ad break
---

# Station imaging

Imaging is the short audio that gives the station its sound: jingles, sweepers, bumpers, station IDs and the music beds under a voice. In RIARS, imaging is organised as categories with an imaging **Kind** and placed in clocks or added by the generator's own rules.

## Jingles every N songs

In **Station settings**, choose a **Jingle category** and a number in **Jingle every N songs (0 = never)**. With, say, 3, the generator puts one jingle from that category after every three songs. Set it to **None** or 0 to stop this.

## Sweepers on a style change

The **Sweeper category on style change** setting adds a sweeper between two songs when their categories differ. It is a way to smooth the change from, for example, pop to rock. If no category is chosen, no sweeper is added.

## Station IDs

A station ID (audio logo) is an imaging element with the kind **Station ID**. Put it in a clock as an **Imaging** slot with a **Time** anchor of `00:00` to place it at the top of the hour. A **Hard** anchor cuts in at the exact second; a soft anchor waits for the current song to finish. A category can also include IDs, but the **Imaging** slot is the clear way to reserve one.

## Ad break frames

An **Ad break** slot can have an **Intro category** and an **Outro category**. When the break has spots, RIARS plays an item from the intro category before the spots and one from the outro category after them. Leave them empty for a bare break. The maximum length is set in **Max seconds**, and the station default is 180 seconds.

## Making the categories

Create categories with the right **Kind** for each purpose, for example **Jingle**, **Sweeper**, **Bumper** and **Station ID**, and add the files to them. The kind affects the colour in the playlist, the imaging rules and the no-overlap setting.

> Tip: keep imaging short and separate from music categories. Mixing them makes separation and the "every N songs" count unreliable.

See [Station settings](topic:station-settings) for the imaging fields and [Format clocks](topic:clocks) for placing imaging in an hour.
