---
id: advertisers-campaigns
title: Advertisers and campaigns
section: advertising
order: 1
keywords: advertising, advertiser, campaign, spots per day, dayparts, max per hour, priority, rotation
---

# Advertisers and campaigns

The **Advertising** tab has three sub-tabs: **Advertisers**, **Campaigns** and **Delivery**. An advertiser is the company that buys airtime; a campaign is one agreement with dates, a daily target, allowed times and a set of spots.

## Advertisers

Click **New**, fill in **Name**, **Contact** and **Notes**, and click **Save**. **Delete** removes the advertiser. You need at least one advertiser before you can create a campaign.

## Campaigns

Choose an advertiser filter or **All advertisers**, then click **New**. In the **Campaign** editor:

| Field | Meaning |
|---|---|
| **Advertiser** | Who the campaign belongs to |
| **Name** | Campaign name |
| **From** and **To** | First and last day it may air |
| **Spots per day** | Daily target; 0 means a pure sponsorship |
| **Max per hour (0 = no limit)** | Ceiling inside one clock hour |
| **Priority** | Higher goes first when a break is full |
| **On** | Enables or disables the campaign |

Under **Dayparts**, add the times when the campaign may air. Each daypart has **Days**, **Start** and **End**. **No dayparts means any time.** Use **Add daypart** and **Remove daypart** to build the allowed windows.

Under **Spots**, use the track picker to add the audio. The table shows **Title**, **Length** and **Weight**; the weight controls how often each spot of the campaign is used. **Save** stores the campaign and **Delete** removes it.

## How spots are spread

The traffic system divides the day into five-minute slices and compares what each campaign has aired with an even pace inside its dayparts. A campaign that is behind gets into the next suitable ad break, first by priority and then by how far behind it is. It may run at most about half a spot ahead of pace. Two spots from the same advertiser do not share a break, and the break's maximum length is respected. Spots never crossfade by default.

See [Sponsorships](topic:sponsorships) and [Delivery report](topic:delivery).
