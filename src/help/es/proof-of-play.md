---
id: proof-of-play
title: Registro de emisiones
section: logs
order: 1
keywords: registro, emisiones, proof of play, historial, log, sgae, derechos, csv, completa
---

# Registro de emisiones

RIARS apunta **todo lo que sale al aire**: música, imagen, cuñas y programas. Es el justificante para los anunciantes y la base de los informes de derechos de autor.

## Qué se apunta

Cada vez que un elemento deja de sonar, se guarda una línea con:

- la **hora real** a la que empezó y acabó;
- el tipo, el título, el artista y el archivo;
- la categoría y la campaña, si las tiene;
- cuánto **sonó** y si fue **completa**;
- el **motivo** del final: completa, saltada, cortada por silencio o detenida.

Una pista cuenta como completa si sonó hasta el final o al menos el 95 %.

## Dónde verlo

- **Registro** (abajo a la izquierda): las últimas emisiones y los avisos de la automatización, en rojo.
- Pestaña **Registro**: el registro completo, con filtros de fechas (**Desde** / **Hasta**), **Tipo** y **Título o artista**, y **Exportar CSV**. Se actualiza solo mientras suena.
- **Archivo ▸ Exportar registro de emisiones (CSV)…**: las últimas 24 horas.
- El [panel web](topic:web-panel) también permite descargarlo.

## Detalles

- Se escribe al acabar cada elemento, de una vez, para no desgastar la tarjeta SD de una Raspberry Pi.
- Si se corta la luz, lo único que puede perderse es la línea del elemento que estaba sonando.
- La rotación de la música también usa el registro: así sabe cuándo sonó cada canción por última vez.

> Consejo: para los informes de derechos de autor, exporta el CSV del mes y filtra por tipo *Música*.
