# Emisión en directo: micrófono, banco de sonidos y modo asistido (plan)

Estado: **propuesta, pendiente de aprobar** (27-10-2026). Rama prevista: `feat/live`, encima de `feat/intro-outro`.

## Objetivo

Que un locutor pueda hacer un programa en directo con RIARS, como en RadioBOSS, Jazler o RadioDJ:
- abrir y cerrar el **micrófono**, con la música bajando sola;
- disparar **jingles, efectos y cortinillas** desde un banco de botones;
- llevar la lista **a mano o en modo asistido**, sin que la automatización le pise.

Todo debe ser seguro en antena. Un fallo del micrófono o del banco nunca puede parar la emisión.

## 1. Canal de micrófono

**Entrada**
- Tarjeta de entrada elegida en **Reproducción ▸ Tarjetas de sonido…**, junto a las de antena y preescucha.
- Por defecto, la del sistema.

**Camino de la señal**
- Un stream de entrada `cpal` escribe en un `rtrb` (sin bloqueos), como ya hacen los reproductores.
- El hilo de salida lo lee y lo mezcla.
- Si las frecuencias no coinciden, se remuestrea.
- Latencia objetivo: menos de 20 ms añadidos.
- Si la tarjeta de entrada falla o se desenchufa, el canal queda en silencio y avisa. La salida sigue.

**Procesado (ligero, apto para Raspberry Pi)**
- Ganancia.
- Filtro paso alto a 80 Hz, contra golpes y zumbido.
- Puerta de ruido opcional.
- Compresor suave opcional.
- El limitador que ya existe a la salida protege de picos.

**Botón MIC**
- ON y OFF con fundido de 150 ms, sin chasquidos.
- Atajo **F9**, configurable.
- Piloto rojo **EN EL AIRE** muy visible.
- Al abrir el micrófono, la música baja **12 dB** (ajustable) con las mismas rampas del autoducking. Al cerrarlo, vuelve.
- Opción **ducking por voz**: la música solo baja mientras se habla, y no mientras el micrófono está abierto en silencio.

**Medidor**
- Nivel del micrófono, en el mismo estilo que el medidor VU.

**Escucha**
- RIARS no devuelve el micrófono por los auriculares de preescucha, para evitar eco y retardo.
- La escucha del locutor va por la mesa o por el monitor directo de la interfaz de audio, como es habitual.

## 2. Banco de sonidos (*cartwall*)

**Rejilla**
- Rejilla de **4 × 6 botones**, con **8 páginas**.
- Cada botón guarda:
  - archivo;
  - texto;
  - color;
  - ganancia;
  - modo: *disparar*, *disparar/parar* o *bucle*;
  - si baja la música (*ducking*) o no.
- Se rellena arrastrando desde la biblioteca, o con clic derecho ▸ **Asignar pista…**.

**Disparo**
- Instantáneo: los sonidos de menos de 30 s se cargan **ya decodificados en memoria** al abrir la página.
- Los más largos se leen del disco como cualquier pista.
- Coste en memoria: unos 10 MB por cada 30 s en estéreo; con una página llena de efectos cortos son pocos MB.

**Mezcla**
- Hasta **8 sonidos a la vez**.
- Van por encima de la música, como el *overlay* actual, sin quitarle el turno a la lista.
- Cada botón muestra el tiempo que le queda mientras suena.

**Teclado**
- Teclas **1–9, 0** y la fila **Q–P** para los botones de la página visible, cuando el foco no está en un campo de texto.
- **Re Pág / Av Pág** cambia de página.
- **Esc** para todos los sonidos del banco.

**Datos y registro**
- Los bancos se guardan en la base de datos de la biblioteca y pueden tener nombre (uno por programa).
- Todo lo que suena desde el banco queda en el **registro de emisiones**.

## 3. Modos de la lista: automático, asistido y manual

| Modo | Qué hace |
|---|---|
| **Automático** (el actual) | La lista avanza sola y la automatización la rellena. |
| **Asistido** | La lista avanza sola, pero la automatización **no añade nada**: suena lo que el locutor ha preparado. |
| **Manual** | Al acabar cada elemento, se **para** y espera a que el locutor pulse **Siguiente** (F8). |

Otras opciones:
- **Parar tras esta pista**: una casilla por elemento, útil para entrar a hablar.
- Al volver a **Automático**, la automatización rellena a partir de donde está la lista. No reescribe lo que hay.

## 4. Panel En directo

Una pestaña nueva, **En directo** (Ctrl+8), con:
- el botón **MIC** grande, el piloto EN EL AIRE, el medidor y la ganancia del micrófono;
- el **banco de sonidos** con sus páginas;
- la **cuenta atrás de la intro**: cuando empieza una canción con marcas, muestra los segundos que quedan hasta que **entra la voz** (dato que ya existe desde la fase de intros), para hablar encima sin pisarla;
- el tiempo restante de la pista, el elemento siguiente y el selector de modo (Automático, Asistido o Manual).

## 5. API, MCP y servidor

**REST y WebSocket**
- Estado del micrófono (abierto o cerrado, nivel).
- Disparar y parar botones del banco.
- Cambiar de modo.
- Eventos `mic` y `cart` en `/ws/events`.

**MCP**
- `get_live_status`, `fire_cart`, `stop_carts` y `set_play_mode`.
- Abrir el micrófono por API o MCP queda **desactivado por defecto**, porque un agente no debería abrir un micrófono sin que lo sepa el locutor. Se activa en los ajustes.

**riars-server**
- El núcleo es el mismo, así que el micrófono y el banco funcionan también sin pantalla, por ejemplo una Raspberry Pi con micrófono USB.
- Se controlan desde el panel web o la API. El panel web tendrá el banco y el botón MIC en una fase posterior.

## Fuera de esta fase (propuestas para después)

- **Grabación del programa** (*logger*): el mix de antena a MP3 por horas.
- **Voice tracking**: grabar locuciones entre canciones para emitirlas después.
- Control con **MIDI o GPIO** (botones físicos y mesas).
- Llamadas telefónicas e invitados remotos.

## Reparto del trabajo

| Quién | Qué |
|---|---|
| Claude | Canal de micrófono en el hilo de audio, mezcla del banco, rampas, modos de la lista, revisión de todo y traducciones |
| DeepSeek, con especificación | Almacenamiento de los bancos, rutas REST y herramientas MCP, panel En directo y tests |
| Agente local | Datos de prueba (bancos de ejemplo), borradores en inglés de commits y del PR, resúmenes de registros |

## Pruebas

- **Tests del mezclador:** micrófono sintético, ducking, 8 sonidos simultáneos, y que un fallo de entrada no pare la salida.
- **Prueba completa contra el servidor:** banco disparado por API y registro de emisiones.
- **Interfaz por CDP:** sin sonido real en casa.
- **CPU y RAM** con micrófono y banco activos.
