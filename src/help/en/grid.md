---
id: grid
title: The grid
section: automation
order: 4
keywords: grid, daypart, rule, priority, days, months, weeks, dates, exceptions, week view
---

# The grid

The **Grid** decides which clock or saved playlist plays at each moment. It is a set of rules, shown over a normal week of 7 × 24 hours. Each coloured cell belongs to the rule that wins for that hour; the colour is the clock's colour, or blue for a playlist. Hours with no rule show the **Default clock**, or a dash if there is none.

## The week view

Use the week arrows (**Previous week** and **Next week**) or the date box to move around. Click a coloured cell to select its rule and edit it below. Hover over a cell to see the rule name, its priority and what it plays.

## Creating a rule

Click **New**, then fill in:

| Field | Meaning |
|---|---|
| **Name** | A label for the rule |
| **Priority** | Higher wins when rules overlap |
| **Plays** | Choose **Clock** or **Playlist**, then the item itself |
| **Start** and **End** | Daily time span |
| **From** and **To** | Optional date range |
| **Enabled** | Whether the rule applies |
| **Days** | Monday to Sunday checkboxes |
| **Months** | January to December checkboxes |
| **Weeks of the month** | **1st** to **5th**, and **Last** |

The hint in the editor is important: **Start equal to end means all day; end before start crosses midnight.** A rule from 22:00 to 02:00 covers the late show across midnight.

Click **Save** to store the rule. **Delete** removes it, and **New** starts a fresh one.

## Priority and exceptions

For a given moment, the enabled rule with the highest **Priority** wins. If two rules have the same priority, the more specific one wins. A date-limited rule with a high priority is the usual way to make an exception, for example a Christmas clock on 25 December, or a summer schedule in July and August.

If no rule applies, the **Default clock** from [Station settings](topic:station-settings) is used. If that is empty too, the automation adds nothing and the current playlist simply continues.

## Resolution by hour

The grid is resolved at the start of each clock period. A 60-minute clock fills the hour; a 30-minute clock repeats twice. When a rule changes, the generator applies it from the next item it builds, so what is already on air is not cut.

See [Format clocks](topic:clocks) for what the rules play and [Preview](topic:preview) to check the result.
