---
id: troubleshooting
title: Solución de problemas
section: reference
order: 2
keywords: problemas, error, no suena, silencio, no conecta, avisos, ayuda, troubleshooting, falla
---

# Solución de problemas

## No suena nada

- Comprueba que el estado sea **AL AIRE** y el volumen no esté a 0.
- Si el medidor VU se mueve pero no oyes nada, el problema está en la salida del sistema (altavoces, mezclador, dispositivo de audio de Windows o macOS).
- La lista puede estar vacía o acabada: añade pistas o activa la [automatización](topic:automation-overview).

## Salta la alarma de silencio

- Algún archivo tiene silencio largo al final o es muy flojo. Sube los segundos o baja el umbral en **Reproducción ▸ Detector de silencio…**, o recorta el archivo.
- Si tu programación tiene pausas a propósito, usa **Solo avisar**. Consulta [Detector de silencio](topic:dead-air).

## La emisión por Internet no conecta

- **ERROR DE STREAM**: revisa el servidor, el puerto, el punto de montaje, el usuario y la contraseña con tu proveedor.
- Comprueba que el ordenador tiene salida de audio: en el escritorio, la emisión la necesita.
- Prueba con 128 kbps.
- Consulta [Emisión por Internet](topic:streaming).

## La automatización no añade nada

1. ¿Está **ACTIVA** en la tarjeta *Automatización*?
2. Mira el último aviso de la tarjeta y los avisos en rojo del **Registro**. Dicen la causa, por ejemplo «la categoría N no tiene nada que poner» o «el reloj X no existe».
3. Comprueba que hay un **Reloj por defecto** o una regla de parrilla para la hora actual.
4. Comprueba que las categorías del reloj tienen pistas (tabla **Pistas** en **Categorías**).
5. Mira **Automatización ▸ Vista previa…**: si está vacía, el problema está en la configuración.

## Se repiten mucho las canciones

La categoría es pequeña para la separación pedida, y el generador la relaja para no quedarse callado. Añade más pistas o baja los minutos de separación.

## Faltan cuñas (Perdidas)

Los bloques de anuncios son cortos o hay pocos para las cuñas contratadas. Alarga **Segundos por bloque de anuncios** o añade bloques a los relojes. Consulta [Cumplimiento y facturación](topic:delivery).

## La lista se para al final

Sin automatización, la reproducción se detiene al acabar la lista. Activa la automatización o añade más pistas.
