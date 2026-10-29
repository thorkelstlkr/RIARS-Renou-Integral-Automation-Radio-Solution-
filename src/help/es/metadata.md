---
id: metadata
title: Completar metadatos
section: onair
order: 10
keywords: metadatos, etiquetas, tags, artista, álbum, año, género, carátula, portada, musicbrainz, acoustid, huella, identificar, completar
---

# Completar metadatos

RIARS puede buscar en catálogos abiertos los datos que les faltan a las pistas de la [biblioteca](topic:library): **título, artista, álbum, año, género y carátula**. Buscar no cambia nada. Ves lo que se ha encontrado al lado de lo que tiene cada pista, y solo se aplica lo que marques.

Rellenar el año y el género sirve también para la automatización: las [categorías](topic:categories) inteligentes pueden filtrar por ellos.

## Abrirlo

- **Clic derecho ▸ Completar metadatos…** en la biblioteca: busca las pistas seleccionadas.
- **Editar ▸ Completar metadatos de la biblioteca…**: dice cuántas pistas de un minuto o más tienen algún hueco (sin artista, álbum, género o año) y las busca al pulsar **Buscar**. Las pistas más cortas, como cuñas y caretas, se dejan fuera porque no están en ningún catálogo.

La búsqueda va en segundo plano. Puedes cerrar el diálogo y volver a abrirlo para ver cómo va. **Detener** la para tras la pista actual y conserva lo encontrado.

## De dónde salen los datos

- **[MusicBrainz](https://musicbrainz.org)**: enciclopedia musical abierta, con datos de dominio público (CC0). RIARS busca la grabación por **título y artista** de las etiquetas y compara la duración. Si la pista no tiene etiquetas pero el archivo se llama «Artista - Título», busca por el nombre.
- **[AcoustID](https://acoustid.org)**, opcional: reconoce la canción por **cómo suena** (su huella de audio), aunque el archivo no tenga ninguna etiqueta. Necesita una clave de aplicación, que se escribe arriba en el diálogo y se guarda con la biblioteca.
  - Es gratis para uso no comercial: regístrala en acoustid.org/new-application.
  - Una emisora comercial necesita su propia clave de acoustid.biz.
- **[Cover Art Archive](https://coverartarchive.org)**: la carátula del álbum.

MusicBrainz admite una petición por segundo, así que cada pista tarda de 2 a 4 segundos: unas 500 pistas son unos 20 minutos.

Cuando la misma canción aparece en varios discos (el original, un directo, un recopilatorio), RIARS prefiere el álbum que ya tiene la pista. Si no tiene ninguno, se queda con el álbum de estudio más antiguo.

## Revisar y aplicar

Cada pista encontrada muestra la carátula, cómo se encontró (por etiquetas, por nombre de archivo o por huella) y el grado de confianza. Debajo aparece cada campo que cambiaría, con el valor actual → el nuevo.

- Vienen marcados **solo los huecos**: campos vacíos, o un título que es el nombre del archivo. Lo que la pista ya tiene nunca se marca solo; si quieres cambiarlo, márcalo tú.
- **Aplicar** guarda lo marcado en la biblioteca. Cada pista recuerda de dónde vino el dato (`musicbrainz:` y el identificador de la grabación).
- **Escribir también las etiquetas en los archivos de audio**, desactivado por defecto, guarda además los valores y la carátula dentro del archivo. Así los ven otros programas. Pide confirmación. RIARS escribe una copia, comprueba que se lee bien y solo entonces sustituye el original. Si el archivo está sonando, puede que no se pueda sustituir, y entonces lo dice.
- Las carátulas se guardan en la carpeta `riars_covers`, junto a la base de datos de la biblioteca.

Las pistas sin coincidencia aparecen al final, plegadas, con el motivo si hubo un error (por ejemplo, sin conexión).

## Para agentes y la API

La API REST tiene `POST /api/v1/library/metadata/lookup` para empezar, `GET` en la misma ruta para el estado y las propuestas, `POST /library/metadata/apply` y `/library/metadata/settings` para la clave. El servidor MCP ofrece `lookup_track_metadata`, `metadata_lookup_status`, `apply_track_metadata`, `get_track_metadata` y `set_acoustid_key`. Ver [API y agentes](topic:api-agents).
