---
id: web-panel
title: Panel web
section: remote
order: 1
keywords: panel, web, navegador, móvil, remoto, token, control remoto, browser
---

# Panel web

El servidor sin pantalla (`riars-server`) incluye un **panel de control en el navegador**. Desde el móvil o desde cualquier ordenador de la red puedes ver qué suena y manejar la emisora.

## Entrar

1. Abre en el navegador `http://<servidor>:3100/`. Por ejemplo, `http://192.168.1.50:3100/`.
2. Escribe el **token de la API**: el que pasaste con `--api-token`, o el que el servidor muestra al arrancar si no pusiste ninguno.
3. Pulsa **Entrar**.

## Qué tiene

- **Lo que suena**, con progreso, botones de reproducción y volumen, y el estado de la emisión por Internet.
- **Lista de reproducción**: toca un elemento para ponerlo en antena.
- **Biblioteca**: búsqueda y botón **+** para añadir a la lista.
- **Automatización**:
  - el interruptor **ACTIVA / PARADA**;
  - el reloj y la franja que se prepara;
  - los avisos;
  - **Lo próximo**: la vista previa de la próxima hora;
  - **Publicidad de hoy**: contratadas, emitidas y perdidas por campaña;
  - **Descargas**: el registro de emisiones, el cumplimiento y las emisiones de cuñas, en CSV.

El panel se actualiza solo (por WebSocket) sin recargar la página. Está pensado para funcionar bien en el móvil.

> Advertencia: el token da control total de la emisora. No lo compartas. Si abres el puerto a Internet, hazlo a través de una VPN o de un proxy con HTTPS.

Consulta también [Servidor sin pantalla](topic:headless-server).
