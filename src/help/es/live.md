---
id: live
title: Emisión en directo
section: onair
order: 9
keywords: directo, micrófono, mic, banco de sonidos, cartwall, botonera, jingles, efectos, modo manual, asistido, en el aire, locutor, eco, monitor directo
---

# Emisión en directo

La pestaña **En directo** (**Ctrl+8**) reúne lo que necesita un locutor: el **micrófono**, el **banco de sonidos** y el **modo** en que avanza la lista.

## Micrófono

- El botón **MIC** (o **F9** desde cualquier pestaña) lo abre y lo cierra, con un fundido corto para que no se oiga ningún golpe. Mientras está abierto, el botón se pone rojo y aparece **EN EL AIRE**.
- Al abrirlo, la música **baja sola** (12 dB por defecto) y vuelve al cerrarlo. Si prefieres que solo baje mientras hablas, elige **Mientras se habla** en los ajustes.
- El medidor muestra el nivel aunque el micrófono esté cerrado: así compruebas la voz antes de salir al aire. Procura que los picos queden en verde o amarillo.
- Si la tarjeta del micrófono falla o se desenchufa, el micrófono se queda en silencio y aparece el error en rojo. La emisión sigue.

**Reproducción ▸ Micrófono…** (o el botón ⚙):

| Ajuste | Por defecto |
|---|---|
| Tarjeta de entrada | La del sistema |
| La tarjeta ya saca el micrófono (monitor directo) | No |
| Ganancia | 0 dB |
| Filtro de graves (corta golpes y zumbidos por debajo de 80 Hz) | Sí |
| Puerta de ruido (silencia el ruido de la sala entre frases) | No |
| Compresor (iguala la voz fuerte y la suave) | Sí |
| Bajar la música | 12 dB, mientras está abierto |
| Permitir abrirlo a la API y a agentes de IA | **No** |

RIARS no devuelve el micrófono por los auriculares de [preescucha](topic:sound-cards): para escucharte, usa la mesa o la escucha directa de tu interfaz de audio.

### Si te oyes con eco

Muchas interfaces de audio (por ejemplo, una Roland UA-4FX) envían la entrada del micrófono directamente a su salida, sin retraso. Si RIARS también lo saca por esa tarjeta, te oyes dos veces: la segunda llega unas décimas tarde y suena a eco. Si la salida de la tarjeta es tu señal al aire, los oyentes también lo oyen.

Marca **La tarjeta ya saca el micrófono (monitor directo)**. RIARS deja entonces de sacar el micrófono por la tarjeta, que ya lo hace sin retraso, y lo añade solo a la [emisión por Internet](topic:streaming). La música sigue bajando al abrir el micrófono y el medidor sigue funcionando. Si tu interfaz no tiene monitor directo, deja la casilla sin marcar: si no, el micrófono no saldría por la tarjeta.

## Banco de sonidos

Una rejilla de **6 × 4 botones** con **8 páginas** para jingles, efectos, cortinillas y camas musicales. Puedes tener varios bancos, por ejemplo uno por programa, y elegir el activo arriba.

- **Asignar un sonido:** arrastra una pista de la biblioteca (o un archivo de la pestaña **Carpetas**) hasta un botón, o haz clic en un botón vacío. En el editor, al pulsar **Elegir una pista…** aparece la lista de la biblioteca, que se filtra mientras escribes; con **Elegir archivo…** puedes usar cualquier archivo de audio del disco, aunque no esté en la biblioteca.
- **Disparar:** clic en el botón, o las teclas **1–0** y **Q–P** para los veinte primeros botones de la página visible. **Re Pág / Av Pág** cambian de página. **Esc** o **Parar todo** paran todos los sonidos del banco.
- Cada botón muestra el tiempo que le queda mientras suena. Pueden sonar hasta **8** a la vez, encima de la música, y también con la lista parada.
- **Clic derecho** sobre un botón: **Editar…** (texto, color, ganancia, modo y si baja la música), **Preescucha** y **Quitar**.

| Modo del botón | Qué hace |
|---|---|
| **Disparar** | Cada pulsación lo empieza desde el principio. |
| **Disparar/parar** | Una pulsación lo empieza y otra lo para. |
| **Bucle** | Se repite hasta que lo paras. Útil para camas musicales. |

Los sonidos cortos (de menos de 30 s) se cargan en memoria al abrir la página, para que suenen al instante. Todo lo que suena desde el banco queda en el [registro de emisiones](topic:proof-of-play).

## Modo de la lista

| Modo | Qué hace |
|---|---|
| **Automático** | La lista avanza sola y la [automatización](topic:automation-overview) la rellena. |
| **Asistido** | La lista avanza sola, pero la automatización **no añade nada ni corta**: suena lo que has preparado. |
| **Manual** | Al acabar cada elemento, la lista **se para** y espera a que pulses **Reproducir** (F5) o **Siguiente** (F8). |

En cualquier modo, **clic derecho ▸ Parar tras esta pista** en la [lista de reproducción](topic:playlist) hace que la emisión se pare al acabar ese elemento. Útil para entrar a hablar. El elemento muestra ⏹.

## Hablar encima de la intro

Cuando empieza una canción con [marcas de intro](topic:intro-outro), el panel muestra en verde los **segundos que quedan hasta que entra la voz**; los tres últimos, en amarillo. Así puedes presentar la canción y callar justo a tiempo.
