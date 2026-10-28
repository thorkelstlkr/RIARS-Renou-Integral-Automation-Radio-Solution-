---
id: headless-server
title: Servidor sin pantalla (Raspberry Pi)
section: remote
order: 2
keywords: servidor, raspberry pi, headless, linux, opciones, línea de comandos, sd, riars-server
---

# Servidor sin pantalla (Raspberry Pi)

`riars-server` es RIARS sin ventana: reproduce, automatiza y emite por Internet, y se maneja desde el [panel web](topic:web-panel), la [API](topic:api-agents) o agentes de IA. Funciona en Linux, en Windows y en una **Raspberry Pi** (64 y 32 bits).

## Arrancarlo

```
riars-server --api-token MI_TOKEN --scan /home/pi/musica
```

## Opciones

| Opción | Para qué | Por defecto |
|---|---|---|
| `--port <n>` | Puerto de la API y del panel web. | 3100 |
| `--api-token <t>` | Token de acceso. También vale la variable `RIARS_API_TOKEN`. Sin él, se genera uno y se muestra al arrancar. | — |
| `--db-path <ruta>` | Base de datos de la biblioteca. | `riars_library.db` |
| `--scan <carpeta>` | Añadir una carpeta a la biblioteca. Todas las carpetas de la biblioteca se reescanean en segundo plano al arrancar. | — |
| `--no-sound-card` | No usar tarjeta de sonido; reproduce con un reloj interno, para servidores que solo emiten por Internet. | — |
| `--device <nombre>` | Tarjeta de sonido de antena, por su nombre. Si no está, se usa la del sistema. | la del sistema |
| `--list-devices` | Mostrar las tarjetas de sonido y salir. | — |
| `--no-resume` | Empezar de cero en vez de seguir donde se quedó. | — |
| `--lang <idioma>` | Idioma de los mensajes: en, es, fr, de, pt, ca. | en |
| `--mcp-stdio` | Funcionar como servidor MCP por la entrada estándar, para agentes de IA. | — |

## Archivos

Junto a la base de datos se guardan la programación de eventos (`riars_schedule.json`) y la sesión (`riars_session.json`). Al reiniciar, el servidor sigue con la misma lista, en la misma posición, con el mismo volumen y la emisión conectada si lo estaba.

## Cuidar la tarjeta SD

RIARS está pensado para equipos modestos:

- La base de datos escribe poco: una línea por elemento emitido.
- La sesión solo se guarda cuando algo cambia, y la posición cada 30 segundos.
- Lo que añade la automatización no provoca escrituras extra.

> Consejo: para que arranque solo con la Raspberry Pi, crea un servicio de systemd que ejecute `riars-server` con tus opciones.
