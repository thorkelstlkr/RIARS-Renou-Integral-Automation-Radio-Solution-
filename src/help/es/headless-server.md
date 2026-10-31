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
riars-server --api-token MI_TOKEN --scan /home/pi/musica --bind 0.0.0.0
```

Sin `--bind`, el servidor solo acepta conexiones del propio equipo. Para usar el [panel web](topic:web-panel) desde el móvil u otro ordenador, pon `--bind 0.0.0.0`.

## Opciones

| Opción | Para qué | Por defecto |
|---|---|---|
| `--port <n>` | Puerto de la API y del panel web. | 3100 |
| `--bind <dirección>` | Dirección en la que escucha la API. `0.0.0.0` abre el servidor a toda la red. | `127.0.0.1` (solo este equipo) |
| `--api-token <t>` | Token de acceso. También vale la variable `RIARS_API_TOKEN`. Sin él, se genera uno la primera vez y se guarda en `riars_api_token`, junto a la base de datos. | — |
| `--media-root <carpeta>` | Carpeta de la que la API y los agentes pueden reproducir, añadir, escanear y etiquetar archivos. Se puede repetir. Las carpetas de la biblioteca siempre cuentan. | solo las carpetas de la biblioteca |
| `--stream-host <servidor>` | Servidor Icecast al que se puede emitir, como `servidor` o `servidor:puerto`. Se puede repetir. | cualquiera |
| `--cors-origin <origen>` | Web de otro dominio que puede usar la API desde el navegador, por ejemplo `https://panel.ejemplo.com`. Se puede repetir. El panel incluido no lo necesita. | ninguna |
| `--db-path <ruta>` | Base de datos de la biblioteca. | `riars_library.db` |
| `--scan <carpeta>` | Añadir una carpeta a la biblioteca. Todas las carpetas de la biblioteca se reescanean en segundo plano al arrancar. | — |
| `--no-sound-card` | No usar tarjeta de sonido; reproduce con un reloj interno, para servidores que solo emiten por Internet. | — |
| `--device <nombre>` | Tarjeta de sonido de antena, por su nombre. Si no está, se usa la del sistema. | la del sistema |
| `--list-devices` | Mostrar las tarjetas de sonido y salir. | — |
| `--no-resume` | Empezar de cero en vez de seguir donde se quedó. | — |
| `--lang <idioma>` | Idioma de los mensajes: en, es, fr, de, pt, ca. | en |
| `--mcp-stdio` | Funcionar como servidor MCP por la entrada estándar, para agentes de IA. | — |

## Archivos

Junto a la base de datos se guardan la programación de eventos (`riars_schedule.json`), la sesión (`riars_session.json`) y el token generado (`riars_api_token`). En Linux, la sesión y el token solo los puede leer el usuario del servidor: la sesión guarda la contraseña de Icecast. Al reiniciar, el servidor sigue con la misma lista, en la misma posición, con el mismo volumen y la emisión conectada si lo estaba.

## Seguridad

- La API va por HTTP sin cifrar. Para manejar la emisora desde fuera de la red local, pon delante un proxy con HTTPS (por ejemplo Caddy o nginx) o usa una VPN.
- Quien tenga el token controla la emisora: guárdalo como una contraseña.
- La API y los agentes solo pueden usar archivos de las carpetas de la biblioteca y de las que indiques con `--media-root`. Las rutas de fuera se rechazan.
- Un detector de intros con programa externo solo se puede elegir en la app de escritorio, nunca por la API.

## Cuidar la tarjeta SD

RIARS está pensado para equipos modestos:

- La base de datos escribe poco: una línea por elemento emitido.
- La sesión solo se guarda cuando algo cambia, y la posición cada 30 segundos.
- Lo que añade la automatización no provoca escrituras extra.

> Consejo: para que arranque solo con la Raspberry Pi, crea un servicio de systemd que ejecute `riars-server` con tus opciones.
