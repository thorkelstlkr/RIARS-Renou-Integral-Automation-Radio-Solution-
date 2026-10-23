# RIARS — Renou Integral Automation Radio Solution

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Software de automatizacion de emisora de radio, open source y multiplataforma. Diseñado para ser gestionado tanto por humanos como por agentes de IA.

## Funcionalidades

- **Reproductor de audio** — MP3, FLAC, WAV, OGG, AAC con crossfade automatico
- **Gestor de playlists** — Crea, edita y organiza listas de reproduccion
- **Programador horario** — Automatiza la emision por franjas horarias y dias de la semana
- **Biblioteca musical** — Escanea carpetas, lee metadatos, busca por titulo/artista/genero
- **Streaming** — Emision en directo a servidores Icecast y Shoutcast
- **Mezclador** — Control de volumen, panorama y medidores VU en tiempo real
- **API REST + WebSocket** — Control remoto completo para integraciones y dashboards
- **Servidor MCP** — Soporte nativo para agentes IA (Claude, etc.) via Model Context Protocol
- **Modo headless** — Funciona sin interfaz grafica en servidores y Raspberry Pi

## Plataformas soportadas

| Plataforma | Modo | Interfaz |
|-----------|------|----------|
| **Linux** x86_64 / ARM | Desktop + Headless | GUI / Terminal |
| **Windows** x86_64 | Desktop | GUI |
| **macOS** Intel + Apple Silicon | Desktop | GUI |
| **Raspberry Pi** (aarch64) | Headless | Terminal + API |
| **Android** | App movil | GUI |

## Arquitectura

```
┌─────────────────────────────────────────────────┐
│                   Agentes IA                     │
│            (Claude, GPT, custom...)              │
│                                                  │
│    ┌─────────────┐    ┌──────────────────┐       │
│    │  MCP Server  │    │    REST API      │       │
│    │ (riars-mcp)  │    │  (riars-api)     │       │
│    └──────┬───────┘    └────────┬─────────┘       │
│           │                     │                 │
│    ┌──────┴─────────────────────┴─────────┐       │
│    │            riars-core                 │       │
│    │  Audio Engine · Playlists · Scheduler │       │
│    │  Media Library · Streaming · Mixer    │       │
│    └──────┬──────────────────────┬────────┘       │
│           │                      │                │
│    ┌──────┴───────┐    ┌────────┴─────────┐       │
│    │ riars-server  │    │   Tauri Desktop   │      │
│    │  (headless)   │    │   + Android App   │      │
│    └───────────────┘    └──────────────────┘       │
└─────────────────────────────────────────────────┘
```

## Stack tecnico

| Capa | Tecnologia |
|------|------------|
| Motor de audio | Rust (`cpal`, `symphonia`) |
| API REST | Rust (`axum`, `tokio`) |
| Servidor MCP | Rust (JSON-RPC stdio/SSE) |
| Base de datos | SQLite (`rusqlite`) |
| Desktop + Android | Tauri v2 |
| Interfaz | React 19 + TypeScript + Vite |

## Inicio rapido

### Desktop (Linux / Windows / macOS)

```bash
# Requisitos: Rust >= 1.77, Node.js >= 20
# Dependencias de sistema para Tauri: https://v2.tauri.app/start/prerequisites/

npm install
npm run tauri dev
```

### Headless (Raspberry Pi / Servidores)

```bash
# Solo necesita Rust — sin Node.js ni GUI
cargo build -p riars-server --release

# Ejecutar (la API exige un token; si no se indica, se genera uno y se muestra al arrancar)
./target/release/riars-server --port 3100 --api-token MI_TOKEN_SECRETO
# o bien: RIARS_API_TOKEN=MI_TOKEN_SECRETO ./target/release/riars-server --port 3100

# Sin tarjeta de sonido (p. ej. un servidor que solo emite a Icecast): reloj interno
./target/release/riars-server --port 3100 --no-sound-card

# Tras un corte de luz o un reinicio, RIARS sigue donde estaba (playlist, posición,
# volumen, crossfade y emisión, guardados en riars_session.json junto a la base de
# datos; incluye la contraseña de Icecast). Para arrancar desde cero:
./target/release/riars-server --port 3100 --no-resume
```

### Controlar con un agente IA

```bash
# Conectar Claude u otro agente via MCP
./target/release/riars-server --mcp-stdio

# O usar la API REST
curl -H "Authorization: Bearer MI_TOKEN_SECRETO" http://localhost:3100/api/v1/status
curl -X POST -H "Authorization: Bearer MI_TOKEN_SECRETO" http://localhost:3100/api/v1/player/play
# WebSocket de eventos: ws://localhost:3100/ws/events?token=MI_TOKEN_SECRETO
```

## Compilar para Raspberry Pi

```bash
# Cross-compilation desde x86_64
rustup target add aarch64-unknown-linux-gnu
cargo build -p riars-server --release --target aarch64-unknown-linux-gnu
```

## Estructura del proyecto

```
Cargo.toml                          Workspace raiz
crates/
  riars-core/                       Motor de audio y logica (sin red, sin GUI)
  riars-api/                        API REST + WebSocket (axum)
  riars-mcp/                        Servidor MCP para agentes IA
  riars-server/                     Binario headless (Raspberry Pi, servidores)
src-tauri/                          App Tauri (desktop + Android)
src/                                Frontend React/TypeScript
```

## Gestion por agentes IA

RIARS esta diseñado para que agentes de IA puedan gestionar una emisora de radio de forma autonoma:

- **Programar la parrilla** — El agente puede crear y modificar la programacion horaria
- **Gestionar playlists** — Crear listas tematicas, reorganizar canciones, ajustar rotaciones
- **Monitorizar la emision** — Comprobar estado del stream, niveles de audio, proximas pistas
- **Reaccionar a eventos** — Actuar cuando una pista termina, el stream se cae o hay silencio
- **Buscar en la biblioteca** — Encontrar canciones por genero, BPM, artista, duracion

Los agentes se conectan via el servidor MCP (para Claude y compatibles) o via la API REST (para cualquier agente HTTP).

## Licencia

[MIT](LICENSE) — Libre para uso personal, comercial, modificacion y distribucion.
