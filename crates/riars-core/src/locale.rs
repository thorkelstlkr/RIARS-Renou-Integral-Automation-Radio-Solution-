use std::collections::HashMap;
use std::sync::RwLock;

static CURRENT_LANG: RwLock<&str> = RwLock::new("en");

type Translations = HashMap<&'static str, HashMap<&'static str, &'static str>>;

fn translations() -> Translations {
    let mut m: Translations = HashMap::new();

    let mut en = HashMap::new();
    en.insert("playback_started", "Playback started");
    en.insert("playback_paused", "Playback paused");
    en.insert("playback_stopped", "Playback stopped");
    en.insert("skipped_next", "Skipped to next track");
    en.insert("volume_set", "Volume set to");
    en.insert("loaded_track", "Loaded track");
    en.insert("scanned_folder", "Scanned folder");
    en.insert("tracks_added", "new tracks added");
    en.insert("unknown_tool", "Unknown tool");
    en.insert("missing_level", "Missing 'level' parameter");
    en.insert("missing_path", "Missing 'path' parameter");
    en.insert("missing_query", "Missing 'query' parameter");
    en.insert("server_starting", "RIARS server starting");
    en.insert("auto_scanned", "Auto-scanned");
    en.insert("scan_failed", "Failed to scan");
    en.insert("mcp_started", "MCP server started (stdio transport)");
    en.insert("api_listening", "RIARS API server listening on");
    en.insert("engine_init_failed", "Failed to initialize audio engine");
    en.insert("library_init_failed", "Failed to initialize media library");
    m.insert("en", en);

    let mut es = HashMap::new();
    es.insert("playback_started", "Reproducción iniciada");
    es.insert("playback_paused", "Reproducción en pausa");
    es.insert("playback_stopped", "Reproducción detenida");
    es.insert("skipped_next", "Avanzado a la siguiente pista");
    es.insert("volume_set", "Volumen ajustado a");
    es.insert("loaded_track", "Pista cargada");
    es.insert("scanned_folder", "Carpeta escaneada");
    es.insert("tracks_added", "pistas nuevas añadidas");
    es.insert("unknown_tool", "Herramienta desconocida");
    es.insert("missing_level", "Falta el parámetro 'level'");
    es.insert("missing_path", "Falta el parámetro 'path'");
    es.insert("missing_query", "Falta el parámetro 'query'");
    es.insert("server_starting", "Servidor RIARS iniciando");
    es.insert("auto_scanned", "Escaneo automático");
    es.insert("scan_failed", "Error al escanear");
    es.insert("mcp_started", "Servidor MCP iniciado (transporte stdio)");
    es.insert("api_listening", "Servidor API RIARS escuchando en");
    es.insert("engine_init_failed", "Error al inicializar el motor de audio");
    es.insert("library_init_failed", "Error al inicializar la biblioteca de medios");
    m.insert("es", es);

    let mut fr = HashMap::new();
    fr.insert("playback_started", "Lecture démarrée");
    fr.insert("playback_paused", "Lecture en pause");
    fr.insert("playback_stopped", "Lecture arrêtée");
    fr.insert("skipped_next", "Piste suivante");
    fr.insert("volume_set", "Volume réglé à");
    fr.insert("loaded_track", "Piste chargée");
    fr.insert("scanned_folder", "Dossier analysé");
    fr.insert("tracks_added", "nouvelles pistes ajoutées");
    fr.insert("unknown_tool", "Outil inconnu");
    fr.insert("missing_level", "Paramètre 'level' manquant");
    fr.insert("missing_path", "Paramètre 'path' manquant");
    fr.insert("missing_query", "Paramètre 'query' manquant");
    fr.insert("server_starting", "Démarrage du serveur RIARS");
    fr.insert("auto_scanned", "Analyse automatique");
    fr.insert("scan_failed", "Échec de l'analyse");
    fr.insert("mcp_started", "Serveur MCP démarré (transport stdio)");
    fr.insert("api_listening", "Serveur API RIARS à l'écoute sur");
    fr.insert("engine_init_failed", "Échec de l'initialisation du moteur audio");
    fr.insert("library_init_failed", "Échec de l'initialisation de la bibliothèque");
    m.insert("fr", fr);

    let mut de = HashMap::new();
    de.insert("playback_started", "Wiedergabe gestartet");
    de.insert("playback_paused", "Wiedergabe pausiert");
    de.insert("playback_stopped", "Wiedergabe gestoppt");
    de.insert("skipped_next", "Zum nächsten Titel gesprungen");
    de.insert("volume_set", "Lautstärke eingestellt auf");
    de.insert("loaded_track", "Titel geladen");
    de.insert("scanned_folder", "Ordner gescannt");
    de.insert("tracks_added", "neue Titel hinzugefügt");
    de.insert("unknown_tool", "Unbekanntes Werkzeug");
    de.insert("missing_level", "Parameter 'level' fehlt");
    de.insert("missing_path", "Parameter 'path' fehlt");
    de.insert("missing_query", "Parameter 'query' fehlt");
    de.insert("server_starting", "RIARS-Server wird gestartet");
    de.insert("auto_scanned", "Automatisch gescannt");
    de.insert("scan_failed", "Scan fehlgeschlagen");
    de.insert("mcp_started", "MCP-Server gestartet (stdio-Transport)");
    de.insert("api_listening", "RIARS-API-Server lauscht auf");
    de.insert("engine_init_failed", "Fehler beim Initialisieren der Audio-Engine");
    de.insert("library_init_failed", "Fehler beim Initialisieren der Medienbibliothek");
    m.insert("de", de);

    let mut pt = HashMap::new();
    pt.insert("playback_started", "Reprodução iniciada");
    pt.insert("playback_paused", "Reprodução em pausa");
    pt.insert("playback_stopped", "Reprodução parada");
    pt.insert("skipped_next", "Avançado para a próxima faixa");
    pt.insert("volume_set", "Volume ajustado para");
    pt.insert("loaded_track", "Faixa carregada");
    pt.insert("scanned_folder", "Pasta analisada");
    pt.insert("tracks_added", "novas faixas adicionadas");
    pt.insert("unknown_tool", "Ferramenta desconhecida");
    pt.insert("missing_level", "Parâmetro 'level' em falta");
    pt.insert("missing_path", "Parâmetro 'path' em falta");
    pt.insert("missing_query", "Parâmetro 'query' em falta");
    pt.insert("server_starting", "Servidor RIARS a iniciar");
    pt.insert("auto_scanned", "Análise automática");
    pt.insert("scan_failed", "Falha ao analisar");
    pt.insert("mcp_started", "Servidor MCP iniciado (transporte stdio)");
    pt.insert("api_listening", "Servidor API RIARS a escutar em");
    pt.insert("engine_init_failed", "Falha ao inicializar o motor de áudio");
    pt.insert("library_init_failed", "Falha ao inicializar a biblioteca de media");
    m.insert("pt", pt);

    let mut ca = HashMap::new();
    ca.insert("playback_started", "Reproducció iniciada");
    ca.insert("playback_paused", "Reproducció en pausa");
    ca.insert("playback_stopped", "Reproducció aturada");
    ca.insert("skipped_next", "Avançat a la pista següent");
    ca.insert("volume_set", "Volum ajustat a");
    ca.insert("loaded_track", "Pista carregada");
    ca.insert("scanned_folder", "Carpeta escanejada");
    ca.insert("tracks_added", "pistes noves afegides");
    ca.insert("unknown_tool", "Eina desconeguda");
    ca.insert("missing_level", "Falta el paràmetre 'level'");
    ca.insert("missing_path", "Falta el paràmetre 'path'");
    ca.insert("missing_query", "Falta el paràmetre 'query'");
    ca.insert("server_starting", "Servidor RIARS iniciant");
    ca.insert("auto_scanned", "Escaneig automàtic");
    ca.insert("scan_failed", "Error en escanejar");
    ca.insert("mcp_started", "Servidor MCP iniciat (transport stdio)");
    ca.insert("api_listening", "Servidor API RIARS escoltant a");
    ca.insert("engine_init_failed", "Error en inicialitzar el motor d'àudio");
    ca.insert("library_init_failed", "Error en inicialitzar la biblioteca de mitjans");
    m.insert("ca", ca);

    m
}

pub const SUPPORTED_LANGUAGES: &[&str] = &["en", "es", "fr", "de", "pt", "ca"];

pub fn set_language(lang: &str) {
    let lang = if SUPPORTED_LANGUAGES.contains(&lang) { lang } else { "en" };
    if let Ok(mut w) = CURRENT_LANG.write() {
        *w = match lang {
            "es" => "es",
            "fr" => "fr",
            "de" => "de",
            "pt" => "pt",
            "ca" => "ca",
            _ => "en",
        };
    }
}

pub fn current_language() -> &'static str {
    CURRENT_LANG.read().map(|r| *r).unwrap_or("en")
}

pub fn t(key: &str) -> String {
    let lang = current_language();
    let all = translations();
    all.get(lang)
        .and_then(|m| m.get(key))
        .or_else(|| all.get("en").and_then(|m| m.get(key)))
        .map(|s| s.to_string())
        .unwrap_or_else(|| key.to_string())
}
