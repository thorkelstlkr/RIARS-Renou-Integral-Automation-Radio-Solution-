---
id: welcome
title: Welcome to RIARS
section: start
order: 1
keywords: RIARS, manual, overview, radio automation, help
---

# Welcome to RIARS

RIARS stands for **Renou Integral Automation Radio Solution**. It is open-source radio automation with native support for AI agents. You can use it to run a station from a desktop computer, or headless from a server or Raspberry Pi.

## What RIARS does

RIARS plays and manages your station:

- a **media library** that scans folders of audio files and searches them;
- a **playlist** with start times, item kinds, saved lists and generated items;
- **playback** with transport controls, volume, seek and **crossfade**;
- a **silence detector** that can skip a silent item or raise an alarm;
- **streaming** to an Icecast server in MP3;
- **scheduled events** for one-off or repeating actions;
- long-term **automation**: categories, format clocks, a dayparts grid and a generator that prepares what airs next;
- **advertising** with advertisers, campaigns, sponsorships and a delivery report;
- a **proof-of-play** log for billing and music reporting.

The desktop app has the full graphical interface. The headless `riars-server` runs the same engine on a server and serves a small browser panel at `http://YOUR-SERVER:3100/`. AI agents can use the REST API or the MCP server.

## How to use this manual

Each topic explains one job. Start with [First steps](topic:first-steps) to add music and get on air, then read [The main window](topic:main-window) to learn where everything is. If you run a station without a screen, read [Web panel](topic:web-panel) and [Headless server](topic:headless-server).

> Tip: press **F1** in the desktop app to open this manual at any time.

Where the desktop app and the browser panel differ, the topic says which one it means. Radio terms are explained in the [Glossary](topic:glossary).
