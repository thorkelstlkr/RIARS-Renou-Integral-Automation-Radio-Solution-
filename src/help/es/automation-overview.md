---
id: automation-overview
title: Automatización: visión general
section: automation
order: 1
keywords: automatización, generador, cómo funciona, activar, por delante, automation, overview
---

# Automatización: visión general

Con la automatización activa, RIARS elige él solo qué suena, hora tras hora, durante semanas, siguiendo tus reglas, igual que los programas profesionales de radio.

## Las piezas

1. **[Categorías](topic:categories)**: grupos de pistas. Por ejemplo *Pop actual*, *Clásicos 80*, *Jingles*, *Indicativos*, *Cuñas*.
2. **[Relojes](topic:clocks)**: plantillas de una hora (o de otra duración) con huecos en orden. Por ejemplo: indicativo en punto, dos canciones de *Pop*, jingle, bloque de anuncios a las :15…
3. **[Parrilla](topic:grid)**: qué reloj, o qué playlist, suena cada hora de cada día. Admite excepciones por fechas.
4. **[Imagen de emisora](topic:imaging)**: jingles cada N canciones, sweepers al cambiar de estilo.
5. **[Publicidad](topic:advertisers-campaigns)**: las campañas llenan los bloques de anuncios de los relojes.

## Cómo elige las pistas

Para cada hueco, el generador toma las pistas de la categoría y descarta las que no cumplen la **separación** (por defecto, no repetir canción en 4 horas ni artista en 45 minutos). Entre las que quedan, prefiere la que **lleva más tiempo sin sonar**, con un poco de azar y teniendo en cuenta su **peso**. Si una categoría es pequeña, relaja la separación poco a poco antes que dejar la emisora en silencio.

Cerca de una hora fija, por ejemplo los anuncios de las :15, elige la duración de las canciones para llegar a tiempo (el *backtiming*).

## Va por delante

El generador no lo decide todo de golpe. Mantiene siempre al menos **3 elementos y 15 minutos** por delante de lo que suena y los añade al final de la [lista de reproducción](topic:playlist). Por eso la tarjeta *Automatización* muestra una franja que está **preparando**, un poco más tarde que la hora actual.

## Activarla

- Botón **PARADA / ACTIVA** de la tarjeta *Automatización*, **Automatización ▸ Automatización activada** o **Ctrl+E**.
- Si la reproducción está parada, pulsa **F5**.

Al desactivarla, lo que ya está en la lista sigue sonando; solo deja de añadir.

## Antes de activarla

Comprueba con **Automatización ▸ Vista previa…** lo que sonaría. Consulta [Vista previa](topic:preview).

> Nota: si algo falla (una categoría vacía, un reloj que no existe), aparece un aviso en amarillo en la tarjeta y en rojo en el registro. Consulta [Solución de problemas](topic:troubleshooting).
