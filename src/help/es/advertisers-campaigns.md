---
id: advertisers-campaigns
title: Anunciantes y campañas
section: advertising
order: 1
keywords: publicidad, anunciante, campaña, cuña, spot, bloque, anuncios, franjas, contratadas, advertising
---

# Anunciantes y campañas

La pestaña **Publicidad** reúne a los clientes y lo que han contratado.

## Anunciantes

En **Anunciantes**, pulsa **Nuevo** y rellena el **Nombre**, el **Contacto** y las **Notas**. Si borras un anunciante, se borran sus campañas, pero lo que ya se emitió sigue en el [registro de emisiones](topic:proof-of-play).

## Campañas

En **Campañas**, pulsa **Nueva**:

| Campo | Significado |
|---|---|
| **Anunciante** y **Nombre** | De quién es y cómo se llama. |
| **Desde** / **Hasta** | Primer y último día, ambos incluidos. |
| **Cuñas al día** | Cuántas veces al día debe sonar. |
| **Máx. por hora** | Tope por hora del reloj (0 = sin tope). |
| **Prioridad** | Si los bloques están llenos, entran antes las de más prioridad. |
| **Franjas** | Días y horas en que puede sonar. Sin franjas, a cualquier hora. |
| **Patrocinio** | Consulta [Patrocinios](topic:sponsorships). |
| **Cuñas** | Los archivos de audio. Si hay varios, se turnan según su **Peso**. |

Pulsa **Guardar**.

## Cómo se reparten

Las cuñas suenan en los **bloques de anuncios** de los [relojes](topic:clocks). En cada bloque, RIARS:

1. Mira qué campañas pueden sonar ahora: dentro de sus fechas y franjas y sin pasar sus topes.
2. Las ordena por **prioridad** y después por lo **retrasadas** que van respecto a un reparto uniforme del día.
3. Mete una cuña de cada campaña mientras quepa en la duración del bloque, y **nunca dos del mismo anunciante** en el mismo bloque.

Así, una campaña de 12 cuñas al día suena aproximadamente cada dos horas, no todas por la mañana. Si un bloque se salta porque pasó su hora, sus cuñas se recolocan solas en los siguientes.

> Nota: las cuñas nunca se funden con lo que va antes o después. Consulta [Crossfade](topic:crossfade).

> Consejo: comprueba el reparto con [Vista previa](topic:preview) y el cumplimiento en [Cumplimiento y facturación](topic:delivery).
