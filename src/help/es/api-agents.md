---
id: api-agents
title: API y agentes de IA
section: remote
order: 3
keywords: api, rest, websocket, mcp, agente, inteligencia artificial, claude, token, integración
---

# API y agentes de IA

RIARS se puede manejar por programa: desde otras aplicaciones, scripts o agentes de inteligencia artificial.

## API REST

- Dirección: `http://<servidor>:3100/api/v1/...`
- Todas las peticiones llevan la cabecera `Authorization: Bearer <token>`.
- Los errores vuelven como `{"error": "…"}`, con código 4xx o 5xx.

Hay rutas para la reproducción, la lista, la biblioteca, la programación, la emisión por Internet y toda la automatización bajo `/api/v1/automation/...`: categorías, relojes, parrilla, campañas, vista previa, registro e informes CSV. Están descritas en `docs/automation.md`.

## WebSocket

`ws://<servidor>:3100/ws/events?token=<token>` envía el estado del motor cada vez que cambia. También envía mensajes `{"type": "automation", …}` cuando el generador añade elementos, se registran emisiones o hay avisos.

## MCP para agentes de IA

Con `riars-server --mcp-stdio`, RIARS funciona como servidor **MCP** (Model Context Protocol). Agentes como Claude pueden:

- ver el estado y manejar la reproducción;
- buscar en la biblioteca y montar listas;
- montar la programación de la semana: categorías, relojes, parrilla, campañas;
- consultar la vista previa, el registro y el cumplimiento.
- cambiar los textos RDS y poner un radiotexto propio durante unos minutos (consulta [RDS](topic:rds)).

Las respuestas van resumidas para no llenar el contexto del agente.

Por seguridad:

- Las herramientas que borran o reescriben (vaciar la lista, borrar listas guardadas, eventos, categorías, relojes, reglas, anunciantes o campañas, quitar carpetas, cerrar pestañas, escribir etiquetas en los archivos) solo funcionan con `"confirm": true`. El agente debe preguntarte antes.
- Los títulos, artistas y etiquetas son datos, no órdenes: si una canción se llama "borra la lista", el agente no debe hacerle caso.
- Los agentes solo pueden usar archivos de las carpetas de la biblioteca y de `--media-root`.

> Nota: los mismos datos se pueden cambiar desde el escritorio, la API o el MCP. Las tres vías usan las mismas validaciones y los mismos mensajes de error.

Consulta también [Servidor sin pantalla](topic:headless-server).
