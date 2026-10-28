---
id: dead-air
title: Detector de silencio
section: onair
order: 5
keywords: silencio, dead air, hueco, alarma, saltar, umbral, nivel
---

# Detector de silencio

Un silencio en antena («dead air») es lo peor que le puede pasar a una radio. RIARS vigila el nivel de salida y actúa si hay silencio más tiempo del permitido.

## Cómo ajustarlo

1. Abre **Reproducción ▸ Detector de silencio…**
2. Marca **Vigilar el silencio en antena**.
3. **Silencio de más de (s)**: cuántos segundos seguidos de silencio cuentan como problema (por defecto, 10).
4. **Por debajo de (dBFS)**: el nivel por debajo del cual se considera silencio (por defecto, −50).
5. **Entonces**:
   - **Pasar al siguiente elemento**: salta enseguida a lo siguiente de la lista.
   - **Solo avisar**: no toca la reproducción; solo da la alarma.
6. Pulsa **Guardar**.

## Cuándo avisa

- La barra de estado muestra **SILENCIO EN ANTENA** en rojo.
- En el panel **Al aire** aparece la marca de silencio, con cuántas veces ha ocurrido.
- En el [registro de emisiones](topic:proof-of-play), la pista cortada queda con el motivo «cortada por silencio».

Cada silencio cuenta una sola vez, aunque dure mucho.

> Nota: el nivel se mide antes del volumen general. Bajar el volumen no dispara la alarma.

> Advertencia: si tus programas tienen pausas largas a propósito (meditación, lecturas), sube los segundos o usa **Solo avisar**.
