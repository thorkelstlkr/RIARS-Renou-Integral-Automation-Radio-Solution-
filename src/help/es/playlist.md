---
id: playlist
title: Lista de reproducción
section: onair
order: 2
keywords: playlist, lista, cola, orden, guardar, abrir, hora de inicio, generado, queue, arrastrar, soltar, carpeta, explorador, pestañas, en antena, programa, insertos
---

# Lista de reproducción

La lista de reproducción contiene lo que suena y lo que viene después. La pista en el aire se marca con una barra roja a la izquierda.

## Columnas

| Columna | Qué indica |
|---|---|
| **#** | Posición. |
| **Inicio** | Hora a la que empezará cada elemento, calculada desde la posición actual. |
| **Título** | Con un punto de color según el tipo: azul música, amarillo imagen (jingle, indicativo, sweeper), rojo cuña, verde promo. |
| **Artista** y **Duración** | Lo que dicen las etiquetas del archivo. |
| **Álbum**, **Año** y **Género** | Los datos de la pista en la [biblioteca](topic:library). Salen con una raya si la pista no está en la biblioteca o le falta ese dato; puedes rellenarlos con [Completar metadatos](topic:metadata). |

## Manejarla

- **Doble clic** en un elemento, o **Intro**: suena ese elemento ahora.
- **▲ / ▼**: subir o bajar (también **Alt+↑ / Alt+↓**). **✕**: quitar (también **Supr**, con varios elementos seleccionados).
- **Ctrl+clic**, **Mayús+clic** y **Ctrl+A** seleccionan varios elementos.
- **Clic derecho**: Reproducir ahora, Reproducir a continuación, Preescucha, Marcas…, Subir, Bajar, Al principio, Al final, Quitar de la lista, Quitar las ya emitidas, Vaciar la lista, Abrir la carpeta del archivo y Copiar la ruta.
- **Guardar** (**Ctrl+S**): guarda con un nombre la lista de la pestaña que ves.
- **Abrir** (**Ctrl+O**): abre una lista guardada en una pestaña nueva; la lista en antena no cambia. Las listas guardadas sirven también en la [parrilla](topic:grid) y en los [relojes](topic:clocks).
- **Limpiar**: vacía la lista.

Al pie ves cuántos elementos hay y cuánto duran en total.

## Pestañas: varias listas a la vez

Encima de la lista hay una pestaña por cada lista abierta. Puedes preparar en una pestaña los insertos de un programa mientras suena otra.

- Solo suena una lista: la que lleva el **punto rojo** (en antena). Las demás se editan igual, con los mismos arrastres, el clic derecho y las marcas, pero no reproducen nada: el doble clic y **Intro** hacen preescucha.
- **+** abre una lista vacía. **Doble clic** en una pestaña le cambia el nombre, y **✕** la cierra (pide confirmación si tiene elementos; guárdala antes si quieres conservarla). La pestaña en antena no se puede cerrar.
- **Añadir a la lista** desde la biblioteca o Carpetas añade a la pestaña que ves. **Reproducir a continuación** va siempre a la lista en antena.
- **Empieza a las**: la hora prevista de inicio de una lista que no está en antena. La columna Inicio calcula desde ella a qué hora sonará cada elemento.
- **Poner en antena** (botón, o clic derecho en la pestaña): la canción que suena termina y después sigue esta lista, sin cortes. La lista que sonaba se queda en su pestaña con lo que le faltaba por sonar, así que al volver a ponerla en antena sigue donde iba.
- **Clic derecho** en elementos: **Copiar a** o **Mover a** otra pestaña y, desde una lista que no está en antena, **Insertar a continuación en antena**, que los pone justo después de lo que suena.

Las pestañas se guardan solas con cada cambio y al abrir RIARS vuelven las mismas.

## Añadir arrastrando

Puedes montar la lista arrastrando:

- **Desde el Explorador de archivos** (o el gestor de archivos de tu sistema): arrastra uno o varios archivos de audio, o una carpeta entera, y suéltalos sobre la lista.
- **Desde la [biblioteca](topic:library)**, en las pestañas de pistas y de carpetas: arrastra una pista a la lista.

Mientras arrastras, una **línea roja** marca dónde entrarán: encima de la fila que tienes debajo del ratón o, en la mitad inferior de la fila, justo debajo. Si sueltas por debajo de la última fila, se añaden al final. Al acercarte al borde de arriba o de abajo, la lista se desplaza sola.

- Los archivos entran en el orden en que los arrastras.
- Una **carpeta** aporta todos sus archivos de audio, también los de sus subcarpetas: primero los de la propia carpeta, por nombre, y luego cada subcarpeta, también por nombre. Las carpetas ocultas y de sistema se saltan, igual que en el árbol de carpetas de la biblioteca.
- Lo que no es audio (imágenes, textos…) se omite. Se admiten MP3, FLAC, WAV, OGG, AAC y M4A.
- Los archivos que aún no están en la biblioteca **se añaden a ella** primero y se analizan en segundo plano, para que tengan [intros y outros](topic:intro-outro) y [normalización](topic:normalization) como el resto.
- Con una carpeta grande, el pie de la lista muestra el avance («Añadiendo a la biblioteca: 120 de 3000») y puedes seguir usando el programa. Al terminar, indica cuántos se añadieron y cuántos se omitieron. Como máximo se añaden 10 000 archivos de una vez.

## Paso entre pistas

Unos segundos antes de que acabe una pista, RIARS ya tiene preparada la siguiente: el paso no deja hueco. Si el [crossfade](topic:crossfade) está activo, las dos se funden.

## Con la automatización activa

La [automatización](topic:automation-overview) añade elementos al final, siempre unos minutos por delante. Al pasar el ratón por el punto de color verás «añadida por la automatización». Puedes mover o quitar lo que quieras: el generador respeta tus cambios y sigue rellenando por el final. Los elementos ya emitidos se van retirando poco a poco para que la lista no crezca sin fin.

> Nota: si cierras el programa, al volver a abrirlo sigue por donde iba, con la misma lista y en la misma posición.
