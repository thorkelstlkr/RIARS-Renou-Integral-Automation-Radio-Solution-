# RIARS — Renou Integral Automation Radio Solution

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Automatización de radio completa, de la parrilla al transmisor. Gratis, open source y multiplataforma: lleva una emisora entera las 24 horas, emite por Internet y por FM con RDS, y si quieres, un asistente de IA puede manejarla contigo.

Web: https://thorkelstlkr.github.io/RIARS-Renou-Integral-Automation-Radio-Solution-/

## Funcionalidades

- **Automatización de largo plazo** — Relojes de formato, parrilla semanal con excepciones por fecha y categorías con reglas; el generador llena la lista durante días o semanas
- **Publicidad** — Anunciantes y campañas con cuñas al día, franjas, máximo por hora y patrocinios, con informes de emisión
- **Directo con micrófono** — La música baja sola al abrir el micro; modos automático, asistido y manual
- **Banco de sonidos** — Ocho páginas de 24 botones para jingles, efectos y cortinas
- **Intros y outros** — El identificativo suena sobre la intro instrumental y termina antes de la voz
- **Normalización** — Sonoridad EBU R128, −16 LUFS por defecto
- **Crossfade y detector de silencio** — Transiciones suaves; salta el elemento si la salida se queda en silencio
- **Pestañas de lista** — Prepara el siguiente programa mientras suena la lista en antena
- **Biblioteca y metadatos** — Escanea carpetas y completa datos y carátulas con MusicBrainz, con tu visto bueno
- **Streaming** — Emite en MP3 a Icecast, incluso sin tarjeta de sonido en modo servidor
- **RDS** — PS, RadioText y RT+ a codificadores externos (UECP o texto, por red, serie o archivo)
- **Codificador estéreo FM** — Genera la señal MPX completa con piloto, estéreo y RDS en 57 kHz, en una tarjeta de 192 kHz
- **API REST + WebSocket y servidor MCP** — Control remoto para paneles, integraciones y agentes de IA (Claude, etc.)
- **Modo headless** — Funciona sin interfaz gráfica en servidores y Raspberry Pi

## Plataformas soportadas

| Plataforma | Modo | Interfaz |
|-----------|------|----------|
| **Linux** x86_64 / ARM | Desktop + Headless | GUI / Terminal |
| **Windows** x86_64 | Desktop | GUI |
| **macOS** Intel + Apple Silicon | Desktop | GUI |
| **Raspberry Pi** (aarch64) | Headless | Terminal + API |
| **Android** | App movil (planeada, aun sin empezar) | GUI |

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
│    │  Media Library · Automation · Mixer   │       │
│    └──────┬──────────────────────┬────────┘       │
│           │                      │                │
│    ┌──────┴───────┐    ┌────────┴─────────┐       │
│    │ riars-server  │    │   Tauri Desktop   │      │
│    │  (headless)   │    │  Android planeado │      │
│    └───────────────┘    └──────────────────┘       │
└─────────────────────────────────────────────────┘
```

Junto a `riars-core` hay tres crates con la parte de red: `riars-stream` (emision a Icecast), `riars-rds` (codificadores RDS) y `riars-meta` (metadatos por Internet).

## Stack tecnico

| Capa | Tecnologia |
|------|------------|
| Motor de audio | Rust (`cpal`, `symphonia`) |
| API REST | Rust (`axum`, `tokio`) |
| Servidor MCP | Rust (JSON-RPC stdio) |
| Base de datos | SQLite (`rusqlite`) |
| Desktop (Android planeado) | Tauri v2 |
| Interfaz | React 19 + TypeScript + Vite |

## Inicio rapido

### Desktop (Linux / Windows / macOS)

```bash
# Requisitos: Rust >= 1.89, Node.js >= 20
# Dependencias de sistema para Tauri: https://v2.tauri.app/start/prerequisites/

npm install
npm run tauri dev
```

### Headless (Raspberry Pi / Servidores)

```bash
# Solo necesita Rust — sin Node.js ni GUI
cargo build -p riars-server --release

# Ejecutar (la API exige un token; si no se indica, se genera uno y se guarda en
# riars_api_token, junto a la base de datos). Por defecto solo escucha en este equipo
# (127.0.0.1); --bind 0.0.0.0 la abre a la red, mejor detrás de un proxy HTTPS.
./target/release/riars-server --port 3100 --api-token MI_TOKEN_SECRETO --bind 0.0.0.0
# La API y los agentes solo usan archivos de la biblioteca y de --media-root:
./target/release/riars-server --api-token MI_TOKEN_SECRETO --media-root /srv/musica
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

# O abrir el panel web desde el móvil u otro ordenador (servidor con --bind 0.0.0.0): http://IP-DEL-SERVIDOR:3100/
# (pide el token una vez y lo recuerda en ese navegador)

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
  riars-stream/                     Emision a Icecast y codificador MP3
  riars-meta/                       Metadatos por Internet (MusicBrainz, AcoustID, caratulas)
  riars-rds/                        Envio de RDS a codificadores externos y salida MPX
  riars-api/                        API REST + WebSocket (axum) y panel web
  riars-mcp/                        Servidor MCP para agentes IA
  riars-server/                     Binario headless (Raspberry Pi, servidores)
src-tauri/                          App Tauri (desktop; Android planeado)
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
