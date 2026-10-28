---
id: streaming
title: Emisión por Internet
section: onair
order: 6
keywords: icecast, shoutcast, stream, emitir, internet, servidor, mp3, bitrate, montaje, mount
---

# Emisión por Internet

RIARS envía lo que suena a un servidor **Icecast** o **SHOUTcast**, desde donde lo escuchan los oyentes.

## Configurar y empezar

1. Pulsa el indicador de estado de la cabecera (pone **DESCONECTADO**), o usa **Emisión ▸ Ajustes de emisión por Internet…**
2. Rellena los datos que te dé tu proveedor:
   - **Servidor** y **Puerto**: por ejemplo, `radio.ejemplo.com` y `8000`.
   - **Punto de montaje**: por ejemplo, `/live` o `/stream.mp3`.
   - **Usuario** y **Contraseña**: normalmente el usuario es `source`.
   - **Bitrate**: 128 kbps es lo habitual.
   - **Nombre de la emisora** y **Género**, que verán los oyentes.
3. Pulsa **Conectar**.

El indicador cambia a **CONECTANDO** y luego a **EMITIENDO**, con los datos enviados.

Para dejar de emitir: **Emisión ▸ Dejar de emitir**.

## Durante la emisión

- El título de cada canción (artista – título) se envía al servidor, y los oyentes lo ven en su reproductor.
- Si se corta la conexión, RIARS reintenta solo.
- Al reiniciar el programa, vuelve a conectarse si estaba emitiendo.

> Nota: por seguridad, la contraseña no se guarda en los ajustes del formulario; se guarda solo con la sesión, para poder reconectar. **Emisión ▸ Empezar a emitir** usa los ajustes guardados si la contraseña ya está puesta.

> Advertencia: si aparece **ERROR DE STREAM**, revisa el servidor, el puerto, el punto de montaje y la contraseña. Consulta [Solución de problemas](topic:troubleshooting).
