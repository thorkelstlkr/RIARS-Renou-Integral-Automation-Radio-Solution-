---
id: imaging
title: Imagen de emisora
section: automation
order: 5
keywords: imagen, jingle, sweeper, bumper, indicativo, station id, cortina, audio logo, imaging
---

# Imagen de emisora

La imagen de la emisora son las piezas cortas que la identifican:

- **Jingles**: piezas cantadas o musicales con el nombre de la radio.
- **Indicativos** (*station ID*, audio logo): «Estás escuchando Radio…», normalmente en la hora en punto.
- **Sweepers**: piezas habladas sobre efectos que unen dos canciones.
- **Bumpers**: entradas y salidas de programas o bloques.

Crea una [categoría](topic:categories) para cada tipo, con su **Tipo** correcto (Jingle, Indicativo, Sweeper, Bumper).

## Tres formas de meter imagen

**1. En el reloj.** Un hueco **Imagen de emisora**, normalmente con hora fija: el indicativo a las `00:00` y a las `30:00`. Consulta [Relojes](topic:clocks).

**2. Reglas automáticas.** Están en [Ajustes de la emisora](topic:station-settings):
- **Jingle cada N canciones**: tras N canciones seguidas suena un jingle de la categoría elegida.
- **Sweeper al cambiar de estilo**: cuando la siguiente canción es de otra categoría que la anterior, suena un sweeper entre las dos.

**3. En los bloques de anuncios.** Cortinas de **entrada** y **salida** que solo suenan si el bloque tiene cuñas.

## Que no se repitan

Dentro de cada categoría de imagen, el generador no repite el mismo elemento hasta que han sonado otros (por defecto, 3). Ajústalo con **Otros elementos de imagen entre repeticiones** en la separación de la categoría.

> Consejo: no abuses. Un jingle cada 3 o 4 canciones y el indicativo en punto suele ser suficiente.
