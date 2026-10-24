---
id: delivery
title: Cumplimiento y facturación
section: advertising
order: 3
keywords: cumplimiento, facturar, factura, informe, csv, perdidas, emitidas, contratadas, delivery, billing
---

# Cumplimiento y facturación

En **Publicidad ▸ Cumplimiento** ves, por día y por campaña, lo contratado frente a lo emitido.

## La tabla

| Columna | Significado |
|---|---|
| **Contratadas** | Las cuñas que tocaban ese día (0 si el día no está en sus franjas). |
| **Emitidas** | Las que sonaron enteras. |
| **Cortadas** | Las que empezaron pero no acabaron, por ejemplo por un salto manual. |
| **Perdidas** | Contratadas menos emitidas, en rojo si hay alguna. |
| **Tiempo** | Los minutos y segundos emitidos. |

Elige las fechas **Desde** / **Hasta** y pulsa **Actualizar**. Al pie están los totales por campaña.

## Exportar para facturar

- **Exportar cumplimiento (CSV)**: la tabla anterior, para la factura o el informe al cliente.
- **Exportar emisiones de cuñas (CSV)**: cada emisión con su hora exacta, opcionalmente solo de un anunciante. Es el justificante que piden muchas agencias.

Los CSV se abren en Excel o LibreOffice. Los nombres con comas van entre comillas, como manda el formato.

## Qué cuenta como emitida

Una cuña cuenta si sonó **hasta el final**, o al menos el 95 % de su duración. Una cuña cortada no se cobra: aparece en **Cortadas**. El registro guarda la hora real de cada emisión, no la prevista.

> Consejo: revisa **Perdidas** cada día. Suele indicar bloques demasiado cortos para las campañas contratadas, o franjas sin bloques de anuncios en el reloj.

Consulta también [Registro de emisiones](topic:proof-of-play).
