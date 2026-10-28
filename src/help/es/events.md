---
id: events
title: Programación de eventos
section: scheduling
order: 1
keywords: programación, evento, hora, diario, semanal, intervalo, acción, schedule, event
---

# Programación de eventos

La pestaña **Programación** ejecuta acciones sencillas a horas fijas: conectar a las informativas, empezar a emitir, bajar el volumen de madrugada… Es independiente de la [automatización](topic:automation-overview). Un evento hace **una cosa en un momento**; la [parrilla](topic:grid) decide **qué suena hora a hora**.

## Cuándo

| Opción | Significado |
|---|---|
| **Una vez** | La próxima vez que llegue esa hora. Después el evento se desactiva solo. |
| **Cada día** | Todos los días a esa hora. |
| **Cada semana** | Un día de la semana a una hora. |
| **Cada N minutos** | Repetir cada cierto tiempo. |

## Acciones

| Acción | Qué hace |
|---|---|
| **Reproducir pista** | Añade el archivo a la lista y lo pone en antena en ese momento. |
| **Reproducir playlist** | Carga una playlist guardada y la reproduce desde el principio. |
| **Fijar volumen** | Pone el volumen general en un valor. |
| **Fundido de volumen** | Lleva el volumen a un valor poco a poco, en los segundos indicados. |
| **Iniciar emisión** / **Parar emisión** | Conecta o desconecta la [emisión por Internet](topic:streaming). |

## Crear un evento

1. Escribe un **Nombre**.
2. Elige **Cuándo** y la **Hora**, o los **Minutos** en *Cada N minutos*.
3. Elige la **Acción** y sus datos: **Elegir archivo…**, la playlist, el volumen…
4. Pulsa **Añadir**.

En la lista puedes desactivar un evento con su casilla **Activado** o borrarlo con **Eliminar**.

## Detalles

- Los eventos se comprueban cada medio segundo, con precisión de segundos.
- Si el ordenador estaba suspendido, al despertar no se ejecutan de golpe todos los eventos atrasados; solo los del último minuto.
- Se guardan en `riars_schedule.json`, en la carpeta de datos.
- La barra de estado muestra cuántos eventos activos hay.

> Consejo: para la música de todos los días usa la [parrilla](topic:grid). Deja los eventos para lo puntual: informativos en cadena, cambios de volumen, empezar a emitir.
