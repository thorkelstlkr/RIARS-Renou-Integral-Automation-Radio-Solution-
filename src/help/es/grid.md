---
id: grid
title: Parrilla
section: automation
order: 4
keywords: parrilla, franjas, dayparts, semana, mes, fechas, excepción, prioridad, navidad, grid
---

# Parrilla

La parrilla dice **qué reloj, o qué playlist, suena en cada momento**. Está hecha de **reglas**. En cada hora gana la regla que encaja con mayor **prioridad**; si empatan, la más concreta (la que cubre menos tiempo).

## Una regla

| Campo | Significado |
|---|---|
| **Suena** | Un [reloj](topic:clocks) o una playlist guardada. Una playlist se reproduce en orden y sigue donde se quedó la vez anterior. |
| **Inicio** y **Fin** | La franja del día. Si son iguales, es todo el día. Si el fin es anterior al inicio, pasa de medianoche: una franja de viernes de 22:00 a 02:00 incluye la madrugada del sábado. |
| **Días** | Los días de la semana. |
| **Meses** | Por ejemplo, solo de junio a agosto. |
| **Semanas del mes** | De la 1.ª a la 5.ª, y **Última**: por ejemplo, el último domingo de cada mes. |
| **Desde** / **Hasta** | Fechas concretas, para temporadas y excepciones. |
| **Prioridad** | Un número más alto gana. |
| **Activa** | Para desactivar una regla sin borrarla. |

## Ejemplo de parrilla

1. *Base*: todo el día, todos los días → reloj *Tarde* (prioridad 0).
2. *Mañanas*: de lunes a viernes, de 07:00 a 10:00 → reloj *Despertador* (prioridad 0, gana por ser más concreta).
3. *Noches de rock*: viernes de 22:00 a 02:00 → reloj *Rock*.
4. *Navidad*: del 25-12 al 25-12, todo el día → reloj *Navidad* (prioridad 10).

Las horas que no cubre ninguna regla usan el **Reloj por defecto** de [Ajustes de la emisora](topic:station-settings).

## La vista semanal

Arriba ves la semana en 7 × 24 casillas, cada una con el color y el nombre de la regla que gana. Con las flechas y la fecha puedes ver otras semanas, por ejemplo la de Navidad, para comprobar las excepciones. Haz clic en una casilla para editar su regla.

> Nota: la regla se decide al empezar cada periodo del reloj (normalmente, cada hora). Un cambio en la parrilla se nota a partir de lo próximo que se genere.
