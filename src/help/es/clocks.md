---
id: clocks
title: Relojes
section: automation
order: 3
keywords: reloj, clock, hueco, hora en punto, ancla, hora fija, dura, blanda, relleno, tarta
---

# Relojes

Un reloj es la plantilla de una hora de radio (o de otra duración, entre 5 minutos y 24 horas): una lista de **huecos** en orden.

## Tipos de hueco

| Tipo | Qué pone |
|---|---|
| **Categoría** | N pistas de una [categoría](topic:categories), con o sin ciertos códigos. |
| **Imagen de emisora** | Un jingle, sweeper, bumper o indicativo de una categoría. |
| **Bloque de anuncios** | Las cuñas de las campañas que tocan, con cortina de entrada y de salida opcionales. Consulta [Anunciantes y campañas](topic:advertisers-campaigns). |
| **Playlist** | Una playlist guardada: entera o N elementos, siguiendo por donde se quedó la vez anterior. |
| **Pista fija** | Un archivo concreto, por ejemplo la sintonía de un programa. |
| **Relleno** | Música de una categoría hasta la siguiente hora fija o hasta el final del reloj. |

## Horas fijas

Cualquier hueco puede tener una **Hora fija (mm:ss)** dentro del reloj, por ejemplo `00:00` para el indicativo en punto o `15:00` para los anuncios.

- **Blanda** (por defecto): el hueco empieza al acabar la canción que suena a esa hora. Nunca corta música. **Espera máx. (s)** indica cuánto puede retrasarse. El generador elige la duración de las canciones anteriores para llegar a tiempo.
- **Dura**: a esa hora exacta se corta lo que suene y entra el hueco. Es útil para conectar con informativos en cadena.

Entre dos horas fijas, los huecos sin hora van en orden. Si se acaban antes de la siguiente hora fija, se rellena con la **Categoría de relleno**. Si llega la hora fija y quedan huecos sin poner, se saltan.

## Crear un reloj

1. En **Relojes**, pulsa **Nuevo** y ponle **Nombre**, **Color** y **Duración (min)**.
2. Pulsa **Añadir hueco** tantas veces como necesites y elige el tipo y los detalles de cada uno. Ordénalos con las flechas.
3. Elige una **Categoría de relleno**.
4. Pulsa **Guardar**. Si falta algo, como una categoría que no existe, verás el error en rojo.

El círculo de la derecha dibuja la hora: marcas en las horas fijas y, entre ellas, los huecos en orden.

## Un ejemplo

`00:00` indicativo · 2 × *Pop actual* · jingle · *Clásicos* · `15:00` bloque de anuncios · relleno de *Pop actual* · `30:00` indicativo · … · `45:00` bloque de anuncios · relleno.

> Consejo: crea pocos relojes bien pensados (mañana, tarde, noche, fin de semana) y combínalos en la [parrilla](topic:grid).
