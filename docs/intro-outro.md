# Intros, outros y locución sobre la intro (plan)

Estado: **preparado, sin terminar** (24-10-2026). Se retoma cuando se renueve el uso semanal.

## Objetivo

Marcar en cada canción del catálogo dónde **entra la voz** (fin de la intro) y dónde **acaba** (inicio de la outro). Las marcas pueden venir de la detección automática o fijarse a mano. Con ellas, los jingles e indicativos suenan **encima de la intro o de la outro**, y **nunca encima de la voz**.

## Ya hecho (rama `feat/intro-outro`, encima de `feat/ui-polish`)

`crates/riars-core/src/analysis/cues.rs` contiene:
- la decodificación a mono a 11 025 Hz;
- una FFT propia, sin dependencias nuevas;
- tres indicios de voz, comparados dentro de la misma canción: energía en la banda vocal, modulación silábica de 3 a 7 Hz y armonicidad;
- el inicio y el final audibles;
- la intro y la outro;
- un índice de confianza;
- la forma de onda para el editor.

Tests sobre canciones sintéticas: 5 de 6 pasan. El que falla es `finds_intro_and_outro_of_a_song`: las marcas salen en el sitio correcto, pero la confianza da 0,297 cuando el test pide 0,3. Hay que calibrar la fórmula de confianza (`(contrast - 0.4) / 1.2`) o el umbral del test, y después probar con música real de `D:\Musica`.

## Modelo de datos

La tabla `tracks` ya tiene `intro_ms` y `outro_ms` sin usar desde la fase 1. Faltan:
- `cue_in_ms` y `cue_out_ms`;
- `cues_source`: `auto`, `manual` o `agent`;
- `cues_confidence`.

La forma de onda **no se guarda**: se calcula al abrir el editor, para no escribir en la SD. Las marcas manuales nunca se sobrescriben con detecciones nuevas.

## Detectores (elegibles en los Ajustes de la emisora)

1. **Integrado** (por defecto): el análisis de `cues.rs`. Es gratis, funciona sin conexión y va en la Raspberry Pi, pero es aproximado.
2. **Agente de IA** (petición del usuario): un rasgo `CueDetector` con varias implementaciones.
   - **Por MCP**: herramientas `get_track_cues`, `analyze_track_cues` (devuelve los indicios y la forma de onda resumidos por segundo) y `set_track_cues` (escribe con `cues_source = agent`). Un agente externo (Claude u otro) revisa las canciones dudosas (confianza baja) y fija las marcas. RIARS no necesita claves de API.
   - **Comando externo**: un ajuste `detector_command`, por ejemplo un script con un modelo de separación de voces (Demucs/Spleeter) o un VAD como Silero. RIARS le pasa la ruta y lee un JSON `{intro_ms, outro_ms, confidence}`. Es lo más preciso, a costa de CPU o GPU.
   - **Modelo con audio por API** (opcional, más adelante): enviar un fragmento reducido a un modelo multimodal. Requiere una clave y que el usuario lo active, porque el audio sale del equipo.
3. En todos los casos, el resultado se propone y la persona confirma o corrige en el editor.

## Pasos que quedan

1. **Calibrar la detección** y probarla con canciones reales (Claude).
2. **Almacenamiento**: columnas, `get/set_track_cues` y detección por lotes en un hilo de baja prioridad, con progreso y que salte las manuales (Claude escribe el hilo; DeepSeek el CRUD).
3. **Motor**, la tercera pletina (Claude, tiempo real):
   - una `overlay` en `Decks` que suena encima de `current` y empieza en un fotograma dado: 0 para la intro, `len - overlay_len` para la outro;
   - sin reservar memoria en el callback;
   - registro de emisiones también para el elemento superpuesto.
4. **Generador**:
   - si el siguiente elemento de imagen cabe en la intro de la canción con margen (por defecto 0,5 s), se superpone en vez de ir antes;
   - lo mismo con la outro;
   - ajustes `talk_over` (activar, intro, outro, margen) en `AutomationConfig`.
5. **API, MCP y comandos Tauri** de marcas y detección (DeepSeek, con especificación).
6. **Interfaz**:
   - editor de marcas con la forma de onda y marcas que se arrastran (fin de intro, inicio de outro, inicio y final audibles);
   - botón "Detectar";
   - columnas **Intro** y **Outro** en la playlist, como RadioBOSS;
   - "Detectar intros y outros" para toda la biblioteca o una categoría

   (DeepSeek, con especificación; textos de Claude).
7. **Manual**: tema nuevo "Intros, outros y locución sobre la intro", en español (Claude) e inglés.
8. **Pruebas**:
   - unitarias;
   - de punta a punta con el servidor sin tarjeta, comprobando que el jingle suena sobre la intro y nunca después de la voz;
   - CDP del editor.

## Pendiente de decidir con el usuario

- **Preescucha**: el editor no puede escuchar las marcas sin que suene en antena; hace falta una segunda salida de audio (cue/PFL). Propuesta: dejarlo para una fase con selección de dispositivo de preescucha.
- **Ducking**: bajar la canción unos dB mientras suena el jingle encima. Propuesta: 0 dB por defecto (sin bajar), ajustable.
