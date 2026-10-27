---
id: normalization
title: Normalización del volumen
section: onair
order: 7
keywords: normalizar, volumen, sonoridad, lufs, ebu r128, nivel, replaygain, limitador, loudness
---

# Normalización del volumen

Cada archivo viene masterizado a un nivel distinto: una canción actual suena mucho más fuerte que una de los 80, y un jingle puede estar muy bajo. La normalización iguala la **sonoridad percibida** para que el oyente no tenga que tocar el volumen.

## Cómo funciona

1. El análisis mide la **sonoridad integrada** de cada pista según la norma **EBU R128** (en LUFS) y su pico. Es la misma pasada que detecta las [intros y outros](topic:intro-outro), y basta hacerla una vez por pista.
2. Al reproducir, cada pista recibe una ganancia fija para llegar al **objetivo**: −16 LUFS por defecto, lo habitual en radio por Internet.
3. Una pista floja solo se sube hasta donde su pico lo permite, y nunca más de lo indicado.
4. Un **limitador suave** a la salida evita saturaciones cuando algo se ha subido o cuando suena un jingle encima de una canción. Las pistas que no se han subido pasan intactas.

Las pistas sin analizar suenan tal cual.

## Ajustes

**Reproducción ▸ Normalización…**

| Ajuste | Por defecto |
|---|---|
| Activada | Sí |
| Objetivo | −16 LUFS |
| Subida máxima | 10 dB |
| Bajada máxima | 12 dB |
| Techo de picos | −1 dBFS |

La columna **LUFS** de la [lista de reproducción](topic:playlist) muestra la sonoridad de cada pista. El editor de marcas indica la ganancia que se le aplicará.

> Consejo: analiza toda la biblioteca una vez (**Categorías ▸ Análisis ▸ Analizar biblioteca**). Lo que añadas después se puede analizar con el mismo botón; solo procesa lo nuevo.
