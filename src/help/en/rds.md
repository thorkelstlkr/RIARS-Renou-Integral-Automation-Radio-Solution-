---
id: rds
title: RDS
section: onair
order: 11
keywords: rds, radiotext, rt, rt+, ps, pi, pty, encoder, uecp, fm, station name, serial port, mpx, stereo encoder, pilot, pre-emphasis, 192 khz
---

# RDS

RIARS sends the RDS texts to your **RDS encoder**, the box that puts them into the FM signal (or the one built into the transmitter). Listeners' radios then show the station name and the song playing.

There are two ways:

- an **external encoder** that takes commands over the network, a serial port or a text file;
- the **built-in FM stereo encoder**: RIARS makes the whole FM signal (stereo and RDS) on a sound card, for a transmitter with an MPX input. See below.

## What is sent

- **PI**: the station code, 4 hex digits (for example `E2A1`). It is on your licence.
- **PS**: the name radios show, 8 characters at most. It stays fixed: many countries do not allow a PS that keeps changing.
- **PTY**: the programme type, 0 to 31 (for example, 10 is Pop in Europe). **TP** says the station carries traffic announcements.
- **Music**: the music/speech flag.
- **RadioText (RT)**: up to 64 characters. For songs it comes from the template, for example `{artist} - {title}`. Artist and title also go as **RT+** tags, which some radios use to show them apart.

## Set it up

1. Open **Broadcast ▸ RDS…**
2. Under **Station**, enter the PI, the PS and the PTY.
3. Under **RadioText**, check the texts:
   - **Song text**: the template for songs.
   - **Station text**: what shows with jingles, spots and other items that are not songs, and while nothing plays. Leave it empty to send the PS.
   - **Text while the microphone is on**: for example "Live with Ana". Leave it empty to keep the song's text.
4. Under **Encoders**, click **Add encoder** and choose how it connects:
   - **TCP** or **UDP**: the encoder's IP address and port.
   - **Serial port**: `COM3` on Windows, `/dev/ttyUSB0` on Linux, at the encoder's speed (usually 9600 baud).
   - **Text file**: a full path ending in `.txt`. RIARS rewrites the file on every change, for encoders or software that read it.
5. Choose the **protocol**:
   - **UECP**: the European standard (SPB 490), which most professional encoders understand. **Site** and **Encoder** at 0 reach every encoder on the line.
   - **Text commands**: for encoders with their own commands. Write the commands from the encoder's manual, with these placeholders:
     - `{ps}` (8 characters) and `{ps_trim}` (without trailing spaces);
     - `{rt}`, `{pi}`, `{pty}`, `{tp}` and `{ms}`;
     - for RT+: `{rtp_title_start}`, `{rtp_title_len}`, `{rtp_artist_start}` and `{rtp_artist_len}`.

     `\r` and `\n` are carriage return and line feed. The starting template, `PS={ps}\r\nRT={rt}\r\n`, works with many encoders.
   - **Characters**: **RDS** sends the radios' character table (with accents), **ASCII** takes the accents off, and **UTF-8** is for files and software.
6. Tick **Send RDS to the encoders** and click **Save**.

Each encoder shows its state: **Sending** when the last send worked, **Error, retrying** when it did not. RIARS retries on its own, waiting longer each time, up to a minute. The texts are sent when what plays changes, and every 30 seconds as well, so an encoder that restarted gets them back.

## Built-in FM stereo encoder

Many transmitters have an **MPX** (multiplex, composite) input. RIARS can feed it directly: it makes the stereo signal (the 19 kHz pilot and the 38 kHz subcarrier) and puts the RDS on 57 kHz, so you need neither a stereo encoder nor an RDS encoder.

You need a sound card that plays at **192 kHz** (176.4 kHz if the on-air audio is at 44.1 kHz), connected to the transmitter's MPX input. Use a card with a flat response up to at least 60 kHz; built-in laptop outputs usually filter above 20 kHz and lose the stereo and the RDS.

1. Open **Broadcast ▸ RDS…**, section **FM stereo encoder (MPX)**.
2. Tick **Make the FM signal on a sound card** and choose the card. Only cards that can play at the right rate are listed; **Refresh** looks again.
3. Choose the **pre-emphasis**: **50 µs** in Europe and most of the world, **75 µs** in the Americas and South Korea. Choose **Off** if the transmitter applies it.
4. Check the levels, as a percentage of full deviation (±75 kHz):
   - **Pilot**: 9 % is usual (8 to 10 %).
   - **RDS**: 4 % is usual (3 to 5 %).
   - **Level**: the card's output at full deviation. Start at -3 dB and set the transmitter's MPX input gain so that full deviation is reached, measuring with a modulation meter.
5. **Stereo** off sends mono, without a pilot. **RDS in the signal** off leaves the RDS out.
6. Click **Save**.

The status shows the card, its rate and **gaps in the audio**, if any: they happen when the on-air audio stops reaching the encoder for a moment. RIARS keeps 100 ms of audio buffered to follow the small speed differences between the two cards.

The built-in encoder sends the same PS, RadioText and RT+ as the external encoders, and works whether **Send RDS to the encoders** is ticked or not.

> Note: before going on air, check the deviation and the signal's spectrum with a modulation meter. Overmodulation interferes with other stations.

## A text of your own

Under **On air now** you can type a text that replaces the song's: a phone number, the next programme, a contest. Choose how many minutes it lasts and click **Show**. **Back to automatic** removes it early.

> Note: RadioText has 64 characters at most; longer texts are cut. Characters radios do not have (emoji, other alphabets) show as `?`.

## From the API and agents

- The REST API has `GET` and `PUT /api/v1/rds` for the settings and the status (the `mpx` part is the built-in FM encoder), and `GET /api/v1/rds/mpx/devices` lists the sound cards it can use.
- `POST /api/v1/rds/text` sets a text of your own, with `{"text": "...", "minutes": 10}`, and `DELETE /api/v1/rds/text` removes it.
- AI agents get the tools `get_rds`, `set_rds_config`, `set_rds_text`, `clear_rds_text` and `list_mpx_devices`. See [API and AI agents](topic:api-agents).

On the headless server, the settings are saved in `riars_rds.json`, next to the library database.
