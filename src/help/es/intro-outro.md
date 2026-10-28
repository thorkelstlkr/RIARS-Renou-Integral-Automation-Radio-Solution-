---
id: intro-outro
title: Intros, outros y locución sobre la intro
section: automation
order: 8
keywords: intro, outro, pisar, voz, marcas, cue, talk over, locución, jingle encima, ducking, análisis
---

# Intros, outros y locución sobre la intro

En radio es habitual **pisar la intro**: el jingle o el indicativo suenan encima de la parte instrumental del principio de una canción y acaban justo antes de que entre la voz. RIARS lo hace solo, **y nunca encima de la voz**.

## Las marcas de cada canción

| Marca | Significado |
|---|---|
| **Inicio** | Primer momento con sonido (sin el silencio inicial). |
| **Entra la voz** | Fin de la intro instrumental. |
| **Acaba la voz** | Inicio de la outro instrumental. |
| **Final** | Último momento con sonido. |

Las columnas **Intro** y **Outro** de la [lista de reproducción](topic:playlist) muestran cuántos segundos se pueden pisar al principio y al final.

## Detectarlas

- **Una canción**: botón **✎** en la biblioteca, o clic derecho ▸ **Marcas…**. Se abre el editor con la forma de onda.
- **Toda la biblioteca o una categoría**: en **Categorías**, recuadro **Análisis**, botón **Analizar biblioteca** o **Analizar esta categoría**. Se hace en segundo plano, una canción tras otra y con pausas, para no quitarle CPU a la emisión. Cada canción se analiza una sola vez.

La detección busca dónde aparece la voz por su energía en la banda vocal, el ritmo de las sílabas y su timbre. **Es aproximada**: propone, y tú confirmas o corriges. Cada marca automática lleva un índice de **confianza**; por debajo del mínimo (0,35 por defecto) no se usa para pisar.

## Corregirlas a mano

En el editor:

1. Arrastra las marcas sobre la forma de onda o escribe los tiempos. Con las flechas se mueven 0,1 s; con Mayús + flechas, 1 s.
2. La franja de colores de debajo indica, segundo a segundo, cuánto se parece a una voz.
3. Escucha por la [tarjeta de preescucha](topic:sound-cards): **▶ Últimos 5 s de intro** comprueba que la marca cae justo antes de la voz.
4. Pulsa **Guardar**. Las marcas manuales **nunca** se pisan con un análisis posterior.

Si la canción no tiene intro, marca **Sin intro**.

## Detección con un agente de IA

En el recuadro **Análisis** puedes elegir otro detector:

- **Programa externo**: un programa que separa la voz con inteligencia artificial (por ejemplo, Demucs o Spleeter con un pequeño script). RIARS le pasa cada archivo y lee el resultado. Es más preciso, pero gasta más ordenador. Si tarda más de dos minutos con un archivo, se detiene.
- **Agente por MCP**: un agente como Claude puede revisar las canciones dudosas con `list_tracks_needing_review`, leer la línea de voz de cada una con `analyze_track` y fijar las marcas con `set_track_cues`. Quedan con origen *Agente*. Un agente no puede cambiar las marcas que has puesto a mano. Consulta [API y agentes de IA](topic:api-agents).

## Cómo pisa la automatización

En **Ajustes de la emisora ▸ Locución sobre intros y outros**:

| Ajuste | Por defecto |
|---|---|
| Activar, sobre la intro, sobre la outro | Sí |
| Margen antes de la voz | 500 ms |
| Bajar la canción (*ducking*) | 6 dB |
| Confianza mínima de las marcas automáticas | 0,35 |
| Tipos que pueden pisar | Jingle, indicativo, sweeper, bumper |

Cuando toca un jingle, RIARS mira la canción siguiente. Si el jingle cabe entero en la intro con el margen, suena encima, colocado para **acabar justo antes de la voz**. Mientras suena, la canción baja unos dB: rápido al entrar y suave al salir. Si no cabe, prueba con la outro de la canción anterior. Si tampoco cabe, suena antes, como siempre.

Las cuñas nunca pisan, y las horas fijas duras tampoco. Lo pisado queda en el [registro de emisiones](topic:proof-of-play) con su hora real.
