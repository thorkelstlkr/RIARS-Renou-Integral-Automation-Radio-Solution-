---
id: preview
title: Preview
section: automation
order: 7
keywords: preview, what would air, plan, difference, hours
---

# Preview

Preview shows what the automation **would** put on air, without touching the playlist. Use it to check a new clock, a grid exception or an ad campaign before it runs.

## Where to find it

- In the desktop app, choose **Automation › Preview…**. The preview can cover up to 48 hours.
- In the browser panel, the **Coming up** list shows the next hour, with time, kind and title.
- In the API, use `GET /api/v1/automation/preview?from=YYYY-MM-DDTHH:MM:SS&hours=N`; the MCP tool is `preview_schedule`.

The preview works whether automation is on or off, because it only asks the generator what it would choose. It writes nothing to the playlist or the log.

## Why it can differ from the air

The preview is a plan, not a promise. It can differ from what finally airs because:

- the generator makes random choices among suitable tracks, and the real run has its own history;
- separation depends on what has actually played, including manual plays and skips;
- the operator can move, remove or add items, and the generator respects those edits;
- a hard anchor may cut in at the exact time, while a soft anchor waits for the current song;
- campaign pacing changes as spots air or are missed;
- settings or clocks changed after the preview was made apply to the next generated item.

The browser panel says it plainly: **Preview only. The Playlist card shows what is really on air.**

> Tip: preview after saving a clock or a rule, not before, so the preview uses the values that will really be used.

See [Automation overview](topic:automation-overview) and [The grid](topic:grid).
