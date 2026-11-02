---
id: rds
title: RDS
section: onair
order: 11
keywords: rds, radiotexto, radiotext, rt, rt+, ps, pi, pty, codificador, encoder, uecp, fm, nombre de la emisora, puerto serie, mpx, codificador estéreo, piloto, preénfasis, 192 khz
---

# RDS

RIARS manda los textos RDS a tu **codificador RDS**, el aparato que los mete en la señal de FM (o el que lleva dentro el transmisor). Así las radios de los oyentes muestran el nombre de la emisora y la canción que suena.

Hay dos maneras:

- un **codificador externo** que acepte órdenes por red, por puerto serie o desde un archivo de texto;
- el **codificador estéreo FM integrado**: RIARS genera toda la señal de FM (estéreo y RDS) en una tarjeta de sonido, para un transmisor con entrada MPX. Se explica más abajo.

## Qué se envía

- **PI**: el código de la emisora, 4 dígitos hexadecimales (por ejemplo `E2A1`). Figura en la licencia.
- **PS**: el nombre que muestran las radios, 8 caracteres como máximo. Es fijo: en muchos países no está permitido un PS que vaya cambiando.
- **PTY**: el tipo de programa, de 0 a 31 (por ejemplo, 10 es Pop en Europa). **TP** indica que la emisora da información de tráfico.
- **Música**: el indicador música/palabra.
- **Radiotexto (RT)**: hasta 64 caracteres. Con las canciones se forma con la plantilla, por ejemplo `{artist} - {title}`. El artista y el título también van como etiquetas **RT+**, que algunas radios usan para mostrarlos por separado.

## Configurar

1. Abre **Emisión ▸ RDS…**
2. En **Emisora**, pon el PI, el PS y el PTY.
3. En **Radiotexto**, revisa los textos:
   - **Texto de las canciones**: la plantilla para las canciones.
   - **Texto de la emisora**: lo que se ve con los jingles, las cuñas y los demás elementos que no son canciones, y cuando no suena nada. Si lo dejas vacío, se envía el PS.
   - **Texto con el micrófono abierto**: por ejemplo, «En directo con Ana». Si lo dejas vacío, se queda el de la canción.
4. En **Codificadores**, pulsa **Añadir codificador** y elige cómo se conecta:
   - **TCP** o **UDP**: la dirección IP del codificador y su puerto.
   - **Puerto serie**: `COM3` en Windows, `/dev/ttyUSB0` en Linux, con la velocidad del codificador (normalmente 9600 baudios).
   - **Archivo de texto**: una ruta completa que acabe en `.txt`. RIARS reescribe el archivo en cada cambio, para codificadores o programas que lo lean.
5. Elige el **protocolo**:
   - **UECP**: el estándar europeo (SPB 490), que entienden la mayoría de codificadores profesionales. **Sitio** y **Codificador** a 0 llegan a todos los codificadores de la línea.
   - **Comandos de texto**: para codificadores con órdenes propias. Escribe las órdenes que indique su manual, con estos huecos:
     - `{ps}` (8 caracteres) y `{ps_trim}` (sin espacios al final);
     - `{rt}`, `{pi}`, `{pty}`, `{tp}` y `{ms}`;
     - para RT+: `{rtp_title_start}`, `{rtp_title_len}`, `{rtp_artist_start}` y `{rtp_artist_len}`.

     `\r` y `\n` son el retorno de carro y el salto de línea. La plantilla inicial, `PS={ps}\r\nRT={rt}\r\n`, sirve para muchos codificadores.
   - **Caracteres**: **RDS** envía la tabla de caracteres de las radios (con eñes y acentos), **ASCII** quita los acentos y **UTF-8** es para archivos y programas.
6. Marca **Enviar RDS a los codificadores** y pulsa **Guardar**.

Cada codificador muestra su estado: **Enviando** cuando el último envío ha funcionado y **Error, reintentando** si no. RIARS vuelve a intentarlo solo, cada vez más espaciado, hasta un minuto. Los textos se envían cuando cambia lo que suena, y también cada 30 segundos, para que un codificador que se ha reiniciado los recupere.

## Codificador estéreo FM integrado

Muchos transmisores tienen una entrada **MPX** (múltiplex, señal compuesta). RIARS puede alimentarla directamente: genera la señal estéreo (el piloto de 19 kHz y la subportadora de 38 kHz) y mete el RDS en 57 kHz, así que no hace falta ni codificador estéreo ni codificador RDS.

Necesitas una tarjeta de sonido que funcione a **192 kHz** (176,4 kHz si el audio al aire va a 44,1 kHz), conectada a la entrada MPX del transmisor. Usa una tarjeta con respuesta plana al menos hasta 60 kHz; las salidas integradas de los portátiles suelen filtrar por encima de 20 kHz y se pierden el estéreo y el RDS.

1. Abre **Emisión ▸ RDS…**, apartado **Codificador estéreo FM (MPX)**.
2. Marca **Generar la señal FM en una tarjeta de sonido** y elige la tarjeta. Solo salen las que pueden funcionar a la frecuencia necesaria; **Actualizar** vuelve a buscarlas.
3. Elige el **preénfasis**: **50 µs** en Europa y casi todo el mundo, **75 µs** en América y Corea del Sur. Elige **No** si ya lo aplica el transmisor.
4. Revisa los niveles, en porcentaje de la desviación máxima (±75 kHz):
   - **Piloto**: lo normal es un 9 % (del 8 al 10 %).
   - **RDS**: lo normal es un 4 % (del 3 al 5 %).
   - **Nivel**: la salida de la tarjeta a desviación máxima. Empieza en -3 dB y ajusta la ganancia de la entrada MPX del transmisor hasta llegar a la desviación máxima, midiendo con un monitor de modulación.
5. Sin **Estéreo** se envía mono, sin piloto. Sin **RDS en la señal** el RDS no va.
6. Pulsa **Guardar**.

El estado muestra la tarjeta, su frecuencia y los **cortes en el audio**, si los hay: ocurren cuando el audio al aire deja de llegar al codificador un momento. RIARS guarda 100 ms de audio para seguir las pequeñas diferencias de velocidad entre las dos tarjetas.

El codificador integrado envía el mismo PS, radiotexto y RT+ que los codificadores externos, y funciona esté marcado o no **Enviar RDS a los codificadores**.

> Nota: antes de salir al aire, comprueba la desviación y el espectro de la señal con un monitor de modulación. La sobremodulación interfiere a otras emisoras.

## Un texto propio

En **En antena ahora** puedes escribir un texto que sustituye al de la canción: un teléfono, el próximo programa, un concurso. Elige cuántos minutos dura y pulsa **Mostrar**. **Volver al automático** lo quita antes de tiempo.

> Nota: el radiotexto tiene 64 caracteres como máximo; los textos más largos se cortan. Los caracteres que las radios no tienen (emojis, otros alfabetos) salen como `?`.

## Desde la API y los agentes

- La API REST tiene `GET` y `PUT /api/v1/rds` para los ajustes y el estado (la parte `mpx` es el codificador FM integrado), y `GET /api/v1/rds/mpx/devices` lista las tarjetas de sonido que puede usar.
- `POST /api/v1/rds/text` pone un texto propio, con `{"text": "...", "minutes": 10}`, y `DELETE /api/v1/rds/text` lo quita.
- Los agentes de IA tienen las herramientas `get_rds`, `set_rds_config`, `set_rds_text`, `clear_rds_text` y `list_mpx_devices`. Consulta [API y agentes de IA](topic:api-agents).

En el servidor sin interfaz, los ajustes se guardan en `riars_rds.json`, junto a la base de datos de la biblioteca.
