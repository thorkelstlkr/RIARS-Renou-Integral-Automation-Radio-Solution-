# Intros, outros, locución sobre la intro, preescucha y normalización (plan)

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

## Decidido con el usuario (24-10-2026)

### 1. Varias tarjetas de sonido y preescucha (cue/PFL)

- **Salidas**: una salida de **antena** (la actual) y otra de **preescucha** elegibles por dispositivo en **Reproducción ▸ Salidas de audio…**. Se listan con `cpal` (ya es dependencia) y se guarda el nombre del dispositivo en la sesión. Si el dispositivo desaparece, se avisa y se vuelve al predeterminado.
- **Motor de preescucha**: un segundo `AudioOutput` independiente, con su propio hilo de audio y un solo reproductor (sin playlist, sin registro de emisiones, sin detector de silencio). Nunca comparte búfer con la antena: un error en la preescucha no puede afectar a lo que suena.
- **Usos**:
  - en el editor de marcas: escuchar desde el inicio audible, los últimos segundos de la intro y el paso a la outro;
  - en la biblioteca y la playlist: botón o tecla de preescucha (por ejemplo **Ctrl+P**);
  - en el servidor sin pantalla: no aplica (sin tarjeta).
- **Seguridad**: si la salida de preescucha es la misma que la de antena, se pide confirmación antes de escuchar, porque sonaría en el aire.

### 2. Autoducking (al pisar la intro o la outro)

- Mientras suena un elemento superpuesto, la canción baja **6 dB por defecto** (ajustable de 0 a 20) con **ataque de 150 ms** y **liberación de 400 ms**, para que el jingle se entienda.
- Se hace en el hilo de audio con una rampa de ganancia por muestra, precalculada, sin reservar memoria. Es el mismo mecanismo de rampa que ya usa el volumen.
- Ajustes en **Ajustes de la emisora ▸ Locución sobre intro/outro**: activar, en intro, en outro, margen y dB de ducking.

### 3. Normalización del volumen

- **Análisis de sonoridad** según **EBU R128 / ITU-R BS.1770** (LUFS integrados, con la ponderación K y el umbral de puertas) y el **pico verdadero** aproximado. Se calcula en la misma pasada que las marcas, así que se decodifica una sola vez por canción.
- Se guardan `loudness_lufs` y `peak_dbfs` por pista. La ganancia de normalización se calcula al reproducir: `objetivo - loudness`, limitada para no pasar de −1 dBTP.
- **Objetivo** configurable, **−16 LUFS** por defecto (lo habitual en streaming), con **límite de ganancia** (+10 dB / −12 dB) para no reventar grabaciones muy flojas.
- Se aplica como **ganancia fija por reproductor** (`TrackPlayer::set_gain`), sin coste en tiempo real.
- Los jingles y las cuñas también se normalizan, cada uno con su propio objetivo opcional (las cuñas suelen ir 1-2 dB por encima).
- Opción de usar la **ReplayGain** de las etiquetas si la canción la trae y aún no se ha analizado.
- **Limitador de seguridad** en la salida, un *soft clipper* sencillo en el hilo de audio, para que ninguna combinación (normalización + superposición) sature.
- **Por lotes**: "Analizar biblioteca" (marcas + sonoridad) en segundo plano y con baja prioridad, con progreso. En una Raspberry Pi tarda unos segundos por canción y solo hace falta una vez.
- En la playlist y la biblioteca: columna de sonoridad (LUFS) y la ganancia aplicada.

## Orden de trabajo para el martes

1. Calibrar la detección de marcas y añadir la sonoridad en la misma pasada (`analysis`).
2. Almacenamiento y análisis por lotes.
3. Motor:
   - ganancia por reproductor (normalización);
   - limitador;
   - tercera pletina con ducking;
   - salida de preescucha.
4. Generador: locución sobre la intro y la outro.
5. API, MCP, Tauri y el detector por agente de IA.
6. Interfaz:
   - editor de marcas con preescucha;
   - ajustes de salidas;
   - columnas Intro, Outro y LUFS.
7. Manual, traducciones, pruebas y PR.
