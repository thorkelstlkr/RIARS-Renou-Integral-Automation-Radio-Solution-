# Automatización a largo plazo (diseño)

Estado: **aprobado** (23-10-2026) con las propuestas por defecto del apartado 8. Fase 1 (núcleo) implementada.

## 1. Objetivo

Que RIARS programe la emisora solo durante semanas o meses, como RCS Zetta, Jazler o RadioDJ:

- música por **categorías / estilos**, con rotación y separación de artistas y canciones;
- **relojes horarios** (*format clocks*) con huecos de música, jingles, IDs y bloques de anuncios;
- una **parrilla** (*dayparts*) que dice qué reloj o playlist suena cada hora, día, semana y mes, con excepciones por fecha;
- un **generador** que va eligiendo y encolando las pistas por delante;
- **imagen de emisora** (jingles, bumpers, audio logos, sweepers) según reglas;
- **publicidad y patrocinios** con campañas, cupos y un **registro de emisiones** exportable para facturar.

Todo con el presupuesto de una Raspberry Pi: sin sondeos nuevos, pocas escrituras en la SD, poca RAM.

## 2. Qué ya existe y cómo encaja

| Pieza actual | Papel en la automatización |
|---|---|
| `Playlist` en antena + `AudioEngine::poll` (cada 250 ms) | El generador **añade** elementos al final de la playlist en antena. El motor no cambia: precarga, paso sin huecos, crossfade y silencio siguen igual. |
| `play_index_unlocked` | Sin cambios. |
| `Scheduler` (eventos once/daily/weekly/interval) | Se conserva para acciones puntuales (volumen, empezar a emitir...). La parrilla nueva **no** usa `Scheduler`: se consulta por hora, no dispara eventos. |
| `MediaLibrary` (SQLite) | Se amplía con tablas nuevas (categorías, relojes, parrilla, campañas, registro). |
| `SessionState` | Guarda además si la automatización está activa y por dónde va el reloj. |

El hilo de audio no se toca: el generador corre en el lado de control (el mismo bucle de `auto_advance` del servidor y de la app), fuera del callback, y nunca dentro del lock del motor mientras consulta SQLite.

### 2.1 No partimos de cero: qué copiamos de otros programas

| Programa | Qué tomamos |
|---|---|
| **Rivendell** (libre, GPL) | La cadena **Parrilla → Reloj → Evento → Pistas** de RDLogManager: la parrilla es 7×24 casillas con un reloj cada una; el reloj es una lista de eventos con hora; cada evento elige de un grupo con *scheduler codes* ("debe tener" / "no debe tener"), separación de artista y título, y **inicio temporizado duro o "esperar hasta N min"**. También los *pre-import / post-import* (imagen antes y después de un evento) y el *traffic merge* (los anuncios se encajan en huecos del reloj). |
| **RadioDJ** | **Categorías y subcategorías**, rotaciones con reglas de separación configurables, "sweepers entre pistas" y la lista generada que el operador puede editar. |
| **LibreTime** | **Smart blocks**: una categoría también puede ser un filtro (género = Rock, año 1980–1989, energía ≥ 3) en vez de una lista hecha a mano. |
| **RCS Zetta / GSelector, Jazler** | Rotación ponderada (A/B/C o *power/recurrent/gold*), separación horizontal (no la misma hora que ayer), *backtiming* hacia las horas fijas, y campañas con cupos y *proof of play* para facturar. |

Los nombres de la interfaz siguen los de estos programas (reloj, parrilla, categoría, cuña, bloque) para que alguien que venga de ellos se oriente enseguida.

### 2.2 Cambios necesarios en el motor actual

Una revisión del código (con DeepSeek como segunda opinión, comprobada a mano) encontró cosas del motor que la automatización necesita arreglar primero. Van en la fase 1:

1. **La pista encolada se guarda por índice** (`queued: (usize, …)` en `engine.rs`). Si el operador o el generador borran o mueven elementos durante la precarga, el índice apunta a otra pista. Se cambia a **id del elemento** (`PlaylistItem.id`), y el índice se busca al cambiar de pista.
2. **Una precarga en curso no se puede cancelar**: si `set_playlist` cambia la lista mientras se decodifica, la pista vieja acaba entrando igualmente. Se añade un **número de época** que la precarga comprueba antes de encolar.
3. **No hay un evento "pista empezada / terminada" con motivo** (fin natural, salto, silencio, parada). Hace falta para que el registro de emisiones sea fiable y no duplique tras un reinicio.
4. **Cada cambio de la playlist reescribe `session.json`**. Con el generador añadiendo pistas, esto escribiría en la SD a menudo. Los añadidos del generador no cuentan como cambio a guardar: tras un reinicio, el generador los vuelve a crear.
5. **SQLite sin WAL**: se activan `journal_mode=WAL`, `synchronous=NORMAL` y `busy_timeout`.
6. La biblioteca usa **una sola conexión tras un `Mutex`**, y un escaneo largo la bloquea. El generador trabaja con su propia caché de candidatas, así que un escaneo no lo frena; si falta, se abre una segunda conexión solo de lectura (WAL lo permite).

Otros avisos de la revisión que no son de la automatización (decodificar entera en RAM una pista sin número de muestras, `fsync` al guardar, seguridad del token único, RDS, emisión de respaldo) quedan anotados como tareas aparte.

## 3. Modelo de datos

Todo en la base SQLite de la biblioteca (`riars_library.db`), en un módulo nuevo `riars-core/src/automation/` (el actual `automation.rs` pasa a `automation/events.rs`). Tipos públicos con `Serialize + Deserialize`.

### 3.1 Categorías

```text
categories(id, name, kind, color, parent_id NULL)
track_categories(track_id, category_id, weight DEFAULT 1.0)   -- muchos a muchos
category_rules(category_id, field, pattern)                    -- autoasignación
```

- `kind`: `Music`, `Jingle`, `Bumper`, `StationId` (audio logo), `Sweeper`, `Promo`, `Spot` (cuña), `Program`, `Bed`, `Other`.
- `parent_id` permite subcategorías: *Música › Pop › Pop 80*. Un hueco de reloj que pide *Pop* puede tirar de sus hijas.
- `weight` sirve para la rotación ponderada (A/B/C de las radios: una pista "power" con peso 3 sale tres veces más).
- **Autoasignación**: reglas por `genre`, `artist`, carpeta (`file_path` empieza por) o año. Se aplican al escanear y a petición. Así una biblioteca de 50 000 pistas no hay que clasificarla a mano.
- **Categoría inteligente** (como los *smart blocks* de LibreTime): en vez de pistas asignadas, un filtro que se evalúa al generar.
- **Códigos** (como los *scheduler codes* de Rivendell): etiquetas libres por pista (`lento`, `femenina`, `navidad`, `explícita`). Un hueco puede pedir "debe tener" o "no debe tener" un código, p. ej. *Pop actual sin `explícita`* de 7 a 10 h.

Campos nuevos en `tracks` (migración): `intro_ms`, `outro_ms` (para pisar locución o encadenar), `year`, `energy` (0–5, opcional), `last_played_at`, `play_count`, `enabled`.

### 3.2 Relojes (format clocks)

```text
clocks(id, name, color, duration_min DEFAULT 60)
clock_slots(clock_id, position, kind, params_json)
```

Cada hueco (`ClockSlot`) es uno de:

| Tipo | Parámetros | Ejemplo |
|---|---|---|
| `Category` | categoría, `count` | 1 canción de *Pop actual* |
| `Imaging` | categoría (jingle, sweeper, bumper, ID) | jingle |
| `StationId` | categoría, `at` (minuto) | ID en la hora en punto |
| `AdBreak` | nombre del bloque, `at` (minuto), duración máxima | cuñas a :15 y :45 |
| `Playlist` | playlist guardada, modo (entera / N pistas / hasta llenar) | programa grabado |
| `Track` | ruta | sintonía de un programa |
| `Fill` | categoría | rellenar hasta el final de la hora |
| `Marker` | `at`, `hard`/`soft` | sincronizar con la hora |

Los huecos con `at` son **anclas de tiempo**. Entre anclas, el generador mete los huecos sin hora en orden y ajusta con `Fill`.

❓ **Decisión 1: anclas duras o blandas por defecto.**
- *Blanda* (recomendada, el "esperar hasta N min" de Rivendell): el bloque empieza al terminar la canción que suena a esa hora. Nunca corta música. Si el retraso pasaría de N minutos, se acorta el tramo anterior.
- *Dura*: a la hora exacta se hace fundido y entra el bloque. Hace falta para enlazar con informativos en cadena.
Propuesta: blanda por defecto y dura como opción por hueco. Para acercarse a la hora, el generador elige la última canción de cada tramo por duración (*backtiming*) para que el bloque caiga lo más cerca posible.

### 3.3 Parrilla (dayparts)

```text
grid_rules(id, name, priority, target_kind, target_id,
           weekdays_mask, months_mask, weeks_of_month_mask,
           start_time, end_time, date_from NULL, date_to NULL, enabled)
```

- `target`: un reloj o una playlist guardada.
- Repetición por **hora** (`start_time`–`end_time`, puede cruzar medianoche), **día de la semana** (máscara de 7 bits), **semana del mes** (1.ª … 5.ª y "última"), **mes** (máscara de 12 bits), y **rango de fechas** opcional (temporada de verano, campaña de Navidad).
- **Excepciones**: una regla con fechas concretas y más prioridad (p. ej. "25 de diciembre, todo el día, reloj Navidad").
- Resolución: para un instante dado, gana la regla activa con mayor `priority`; a igual prioridad, la más específica (menos días/horas cubiertas). Si ninguna aplica, se usa el **reloj por defecto** de la emisora; si no hay, la automatización no añade nada y el motor sigue con lo que tenga (comportamiento actual).
- La parrilla se resuelve **por hora de reloj**: al empezar una hora nueva se elige su reloj. Un reloj de 30 min se repite dos veces.

Vista de la parrilla: una semana tipo de 7×24 casillas coloreadas por reloj, más un calendario mensual para ver excepciones.

### 3.4 Publicidad y patrocinios

```text
advertisers(id, name, contact, notes)
campaigns(id, advertiser_id, name, date_from, date_to,
          plays_per_day, max_per_hour, dayparts_json, priority,
          sponsor_of NULL)           -- clock/programa/regla de parrilla patrocinada
campaign_spots(campaign_id, track_id, weight)   -- una o varias cuñas que rotan
play_log(id, aired_at, ended_at, track_id, file_path, title, artist,
         kind, category_id, campaign_id NULL, break_name NULL,
         planned_at NULL, duration_ms, status)  -- proof of play
```

- **Campaña**: anunciante, fechas, emisiones por día (objetivo), máximo por hora, franjas donde puede ir (p. ej. sólo 7–10 h y 18–20 h), prioridad.
- **Bloques**: los huecos `AdBreak` del reloj. Al generar un bloque se eligen cuñas de las campañas activas que van más retrasadas respecto a su objetivo diario (reparto uniforme a lo largo del día), sin dos cuñas del mismo anunciante en el mismo bloque y respetando la duración máxima del bloque. Opcional: cortina de entrada y salida del bloque (categorías de imagen).
- **Patrocinio**: una campaña con `sponsor_of` se emite al principio (y/o final) del programa o franja patrocinada ("Este programa llega gracias a...").
- **Registro de emisiones** (*proof of play*): una fila por elemento que **de verdad sonó**, escrita cuando el motor cambia de pista (no al planificar), con la hora real. `status` = `aired` / `skipped` / `failed`. Exportable a **CSV** por anunciante, campaña y rango de fechas, con totales para facturar. También sirve para las rotaciones musicales y para informes de derechos de autor (SGAE y similares).
- Las cuñas que no cupieron se registran como `missed` en un informe aparte para recolocarlas o compensar.

## 4. Generador

### 4.1 Cuándo corre

- Sin sondeo nuevo. Se engancha al bucle de control que ya existe (`auto_advance`, cada 250 ms) y sólo trabaja cuando:
  - cambia `playlist_revision` (empezó otra pista), o
  - lo que queda por delante baja del **horizonte**.
- **Horizonte** ❓ (Decisión 2): mantener por delante al menos **3 elementos y 15 minutos**. Con eso siempre hay una pista preparada para la precarga y el operador ve qué viene, pero si cambia la parrilla o entra una excepción el plan se corrige pronto. Generar la hora entera de golpe se podría hacer como "vista previa" sin meterla en antena.
- Lo que ya sonó se **poda** de la playlist en antena (deja las últimas 20) para que la RAM y el `session.json` no crezcan sin fin en semanas de emisión continua.

### 4.2 Algoritmo

Para cada hueco pendiente del reloj actual:

1. **Candidatas**: pistas de la categoría (y subcategorías) con `enabled = 1`. Se leen de SQLite con índice por categoría; se cachean por categoría hasta que cambie la biblioteca (una lista de ids, no las pistas enteras: poca RAM).
2. **Filtros de separación** (configurables por categoría, con valores por defecto):
   - misma canción: no antes de **N horas** (p. ej. 4 h; 24 h para recurrentes);
   - mismo artista: no antes de **M minutos** (p. ej. 45 min);
   - mismo álbum / mismo título con otro intérprete (versiones): opcional;
   - no repetir la misma canción en la misma hora del día anterior (*horizontal separation*), opcional;
   - jingles e IDs: no repetir el mismo hasta haber pasado X otros.
3. **Puntuación**: entre las que pasan, se prefiere la que lleva más tiempo sin sonar, multiplicada por el `weight`; con un poco de aleatoriedad para no ser previsible. Rotación tipo "la menos reciente primero" con peso.
4. **Relajación**: si ninguna pasa los filtros (categoría pequeña), se relajan en orden (álbum → horizontal → artista a la mitad → canción a la mitad) en vez de dejar la emisora en silencio. Se anota en el log.
5. **Anclas**: antes de un ancla (`AdBreak` a :15) se calcula el tiempo que queda y se elige la última canción por duración para acercarse a la hora; si sobra o falta poco, el ancla blanda lo absorbe.
6. **Imagen**: reglas del reloj o de la emisora, p. ej.:
   - jingle cada N canciones;
   - sweeper o bumper cuando cambia la categoría o el estilo entre dos canciones;
   - ID de emisora a la hora en punto (y opcionalmente a la media);
   - entrada y salida de los bloques de anuncios.
   Se pueden marcar como "pisar la intro" (el jingle encima de la `intro_ms` de la canción) usando el crossfade existente; en la primera versión sólo se encadenan.
7. **Fallback**: si una categoría está vacía o falla todo, se usa la categoría de relleno de la emisora y, si tampoco hay, se repite en bucle lo último generado (el motor hoy se para al final de la lista porque `RepeatMode` vale `Off` por defecto). **Nunca** se queda sin nada que sonar sin avisar (evento de API y log).

La historia para separar se lee de `play_log` al arrancar (últimas 24 h) y se mantiene en memoria en un anillo pequeño; no se consulta SQLite en cada elección.

### 4.3 Elementos de la playlist

`PlaylistItem` gana campos opcionales (con `#[serde(default)]`, compatibles con sesiones y playlists guardadas): `kind` (música, jingle, cuña...), `category_id`, `campaign_id`, `planned_at`, `track_id` y `generated: bool`. El operador puede borrar, mover o añadir a mano: el generador sólo rellena por el final y respeta lo que haya.

### 4.4 Modos

- **Manual**: como ahora.
- **Automático**: el generador rellena según la parrilla.
- **Asistido** ❓ (Decisión 3, opcional): genera pero el operador confirma cada hora. Propongo dejarlo para más adelante.

Cambiar a automático no corta lo que suena: añade por detrás.

## 5. Rendimiento en Raspberry Pi y SD

- SQLite en `journal_mode=WAL`, `synchronous=NORMAL`: menos escrituras y sin bloquear lecturas.
- Una inserción en `play_log` por pista que suena (≈ 15–20 por hora, unos KB al día). `last_played_at` y `play_count` en la misma transacción.
- Relojes, parrilla y campañas se leen al arrancar y al cambiar; en cada elección no hay E/S salvo la consulta de candidatas cacheada.
- Coste del generador: decenas de microsegundos por elección con listas de ids de unos miles; se medirá CPU, RAM y tiempo por elección en el Pi (o con `--no-sound-card` en el PC) antes y después.
- Purga opcional del `play_log` pasado X meses (por defecto, conservar 13 meses para facturación anual).

## 6. API, MCP e interfaces

- REST `/api/v1/automation/...`: categorías, asignación de pistas (incluida en lote por regla), relojes y huecos, parrilla, campañas, anunciantes, `GET /automation/preview?from=&hours=` (lo que sonaría), `GET /logs/plays.csv?from=&to=&advertiser=`, y activar/desactivar el modo automático.
- Eventos WebSocket: `automation_generated`, `automation_warning` (categoría vacía, cuñas que no caben), `ad_aired`.
- MCP: las mismas operaciones como herramientas (`create_clock`, `set_grid_rule`, `create_campaign`, `preview_schedule`, `export_play_log`...). Pensado para que un agente pueda montar la programación de una semana.
- **Guía visual**: la disposición de RadioBOSS Advanced, que el usuario tomó como referencia. Arriba: reloj, pista anterior, en el aire y siguiente. A la izquierda: biblioteca por carpetas y categorías, con un registro debajo. En el centro: playlist en pestañas con columnas de hora de inicio, duración, repeticiones, intro, outro, valoración y última emisión, y los controles debajo. Abajo del todo: barra de estado con el detector de silencio y el programador.
- **Escritorio** (Tauri + React): pestañas *Categorías*, *Relojes* (editor de huecos con vista de "tarta" de la hora), *Parrilla* (semana tipo + calendario), *Publicidad* (anunciantes, campañas, estado de cumplimiento) y *Registro*.
- **Panel web** (`/`): vista de la parrilla del día, lo que viene, estado de campañas y descarga del CSV. La edición completa queda en el escritorio y la API.

## 7. Fases

Cada fase: rama propia, tests y PR. Se fusiona cuando el usuario lo apruebe.

| Fase | Rama | Contenido |
|---|---|---|
| 1 | `feat/automation-core` | Arreglos del motor del apartado 2.2, categorías (tablas, autoasignación), campos nuevos de pista y de `PlaylistItem`, `play_log`, relojes y parrilla (modelo + resolución), generador con rotación, separación, anclas e imagen; integración en el bucle de control del servidor. Tests unitarios del generador con biblioteca sintética (24 h simuladas sin audio). |
| 2 | `feat/automation-ads` | Anunciantes, campañas, bloques, patrocinios, reparto, informe de fallos y exportación CSV. |
| 3 | `feat/automation-api` | REST, WebSocket y MCP. Pruebas con el servidor sin tarjeta (`--no-sound-card --no-resume`) y un Icecast falso. |
| 4 | `feat/automation-ui` | Pantallas de escritorio y panel web. Pruebas por CDP (WebView2). |

## 8. Decisiones que necesito del usuario

1. **Anclas de tiempo** (anuncios a :15, ID a :00): ¿blandas por defecto (esperan a que acabe la canción) con opción de dura por hueco? *Propuesta: sí.*
2. **Horizonte del generador**: ¿3 elementos / 15 minutos por delante en antena, más una vista previa de horas o días bajo demanda? *Propuesta: sí.*
3. **Modo asistido** (confirmar cada hora): ¿para más adelante? *Propuesta: más adelante.*
4. **Separaciones por defecto**: canción 4 h, artista 45 min, jingle sin repetir hasta pasar otros 3. *¿Te valen?*
5. **Registro**: CSV con hora real, título, artista, anunciante, campaña y duración; conservar 13 meses. *¿Necesitas otro formato (Excel, PDF, el de SGAE)?*
6. **Orden de las fases**: núcleo musical → publicidad → API/MCP → interfaz. *¿O prefieres la publicidad antes?*
