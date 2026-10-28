---
id: library
title: Biblioteca
section: onair
order: 1
keywords: biblioteca, carpeta, carpetas, árbol, explorador, escanear, reescanear, buscar, pistas, música, etiquetas, library, scan, folders
---

# Biblioteca

La biblioteca es la lista de todos los archivos de audio que RIARS conoce. Se guarda en una base de datos dentro de la carpeta de datos del programa.

## Añadir música

La biblioteca se construye a partir de **carpetas de la biblioteca**. RIARS las recuerda y las vuelve a escanear cada vez que arranca, así que la música nueva que copies en ellas aparece sola.

Hay tres formas de añadir carpetas:

- **Pestaña Carpetas**: un árbol del equipo que se despliega con ▸. Marca la casilla de cada carpeta que quieras en la biblioteca; puedes marcar varias seguidas.
- **Añadir carpeta**, encima de la biblioteca, o **Archivo ▸ Añadir carpeta a la biblioteca…** (**Ctrl+Mayús+A**): en el selector puedes elegir varias carpetas a la vez.
- **Añadir…** en la pestaña Carpetas hace lo mismo.

RIARS escanea en segundo plano, con las subcarpetas, y la línea de avisos dice qué carpeta lee y cuántas pistas nuevas encuentra. Mientras tanto puedes seguir usando el programa. Volver a escanear solo añade lo nuevo: las pistas que ya estaban no se duplican y sus etiquetas no se vuelven a leer.

Lee archivos **MP3, FLAC, WAV, OGG, AAC y M4A**. De cada uno toma título, artista, álbum, género y duración de sus etiquetas. Si no tiene etiquetas, usa el nombre del archivo como título.

En el [servidor sin pantalla](topic:headless-server), `--scan` añade una carpeta a la biblioteca al arrancar, y la API REST y el servidor MCP pueden listar, añadir, quitar y reescanear las carpetas de la biblioteca.

## La pestaña Carpetas

Arriba está la lista de **Carpetas de la biblioteca**, con cuántas pistas tiene cada una, y el botón **Reescanear** para buscar música nueva sin reiniciar.

Debajo está el árbol del equipo:

- Una casilla **marcada** es una carpeta de la biblioteca. Si está marcada pero gris, la carpeta ya entra a través de otra carpeta de más arriba.
- Al **desmarcar** una carpeta, o al pulsar ✕ en la lista, RIARS pregunta si quitas solo la carpeta (sus pistas se quedan) o también sus pistas. Los archivos **nunca se borran** del disco.
- Al pulsar el **nombre** de una carpeta, abajo aparecen sus archivos de audio. Los que aún no están en la biblioteca salen atenuados, pero también se pueden usar.

Los archivos de esa lista funcionan como las pistas de la biblioteca: **doble clic** o **Intro** los añade a la lista de reproducción, se pueden **arrastrar** a un botón del [banco de sonidos](topic:live) y tienen menú del botón derecho.

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
