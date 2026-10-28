---
id: sound-cards
title: Tarjetas de sonido y preescucha
section: onair
order: 8
keywords: tarjeta de sonido, salida, dispositivo, preescucha, cue, pfl, auriculares, escuchar, device
---

# Tarjetas de sonido y preescucha

RIARS puede usar dos tarjetas de sonido: una para **antena** (lo que oye la emisora) y otra para **preescucha** (escuchar algo por los auriculares sin que salga al aire).

## Elegirlas

**Reproducción ▸ Tarjetas de sonido…**

- **Antena**: la tarjeta por la que sale la emisión. Por defecto, la del sistema. El cambio se aplica al **reiniciar** RIARS, para no cortar la emisión.
- **Preescucha**: la tarjeta de los auriculares, o *Desactivada*. Se aplica al momento.

> Advertencia: si eliges para preescucha la misma tarjeta que la de antena, lo que escuches **saldrá al aire**. El diálogo te avisa en amarillo.

La preescucha va por su propio motor de audio, separado del de antena: un fallo en ella no puede afectar a la emisión. Si la tarjeta de preescucha desaparece, por ejemplo al desenchufar los auriculares USB, simplemente deja de sonar.

## Preescuchar

- En la biblioteca: clic derecho sobre una pista ▸ **Preescucha**.
- En el [editor de marcas](topic:intro-outro): los botones **▶** para escuchar el principio, el final de la intro o el final de la voz.

La preescucha suena al mismo nivel que sonaría en antena, con la [normalización](topic:normalization) aplicada.

## En el servidor sin pantalla

`riars-server --list-devices` muestra las tarjetas, y `--device "<nombre>"` elige la de antena. El servidor no tiene preescucha. Consulta [Servidor sin pantalla](topic:headless-server).
