---
id: library
title: Biblioteca
section: onair
order: 1
keywords: biblioteca, carpeta, escanear, buscar, pistas, música, etiquetas, library, scan
---

# Biblioteca

La biblioteca es la lista de todos los archivos de audio que RIARS conoce. Se guarda en una base de datos dentro de la carpeta de datos del programa.

## Añadir música

1. Pulsa **Añadir carpeta** encima de la biblioteca, o usa **Archivo ▸ Añadir carpeta a la biblioteca…** (**Ctrl+Mayús+A**).
2. Elige la carpeta. RIARS recorre también las subcarpetas.

Lee archivos **MP3, FLAC, WAV, OGG, AAC y M4A**. De cada uno toma título, artista, álbum, género y duración de sus etiquetas. Si no tiene etiquetas, usa el nombre del archivo como título.

Volver a escanear una carpeta solo añade lo nuevo. Las pistas que ya estaban no se duplican.

## Buscar

Escribe en **Buscar…** (o pulsa **Ctrl+L**). Busca a la vez en título, artista, álbum y género, sin distinguir tildes ni mayúsculas: «cancion» encuentra «Canción». La lista se carga por páginas mientras bajas, así que va rápida aunque tengas decenas de miles de pistas.

## Usar una pista

- **Doble clic** o **Intro**: la añade al final de la [lista de reproducción](topic:playlist).
- **Cargar**: la carga directamente en el reproductor.

Puedes seleccionar varias pistas: **Ctrl+clic** añade o quita una, **Mayús+clic** selecciona un tramo y **Ctrl+A** todas.

## Menú del botón derecho

Clic derecho sobre una pista, o sobre la selección:

| Opción | Qué hace |
|---|---|
| **Añadir a la lista** | Al final de la lista de reproducción. |
| **Reproducir a continuación** | Justo después de lo que suena ahora. |
| **Preescucha** | Por la [tarjeta de preescucha](topic:sound-cards), sin salir al aire. |
| **Marcas…** | Abre el editor de [intros y outros](topic:intro-outro). |
| **Detectar marcas** | Analiza las pistas elegidas; las marcas puestas a mano se conservan. |
| **Abrir la carpeta del archivo** | Muestra el archivo en el explorador. |
| **Copiar la ruta** | Copia la ruta del archivo. |
| **Quitar de la biblioteca…** | Pide confirmación. El archivo **no se borra** del disco, pero la pista sale de sus categorías. También con **Supr**. |

## La biblioteca y la automatización

Para que la automatización use una pista, la pista tiene que estar en una [categoría](topic:categories). La forma más cómoda es crear una **regla por carpeta**: por ejemplo, todo lo que haya en `D:\Música\Pop` va a la categoría *Pop*.

> Consejo: organiza las carpetas por tipo (música, jingles, indicativos, cuñas). Así las reglas lo clasifican todo solas.
