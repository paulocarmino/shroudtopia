//! `<game>/shroudtopia.json`, the loader's config: which mods are active and their options. The loader reloads it
//! when it changes on disk, so a toggle here applies with the game open.
//!
//! Every write reads the file fresh, changes the one value, and writes it back whole (a temp file renamed over it, so
//! the loader never reads half a file), keeping every key the app does not know and the file's key order, with the
//! loader's own 4-space indent. Friendly names live in `CATALOG`; a mod or option not in it still shows, by its key.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map, Value};

pub const CONFIG_FILE: &str = "shroudtopia.json";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptKind {
    Bool,
    Number { default: f64, min: f64, max: f64, step: f64 },
}

pub struct KnownOpt {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: OptKind,
}

pub struct KnownMod {
    pub key: &'static str,
    /// the file under `<game>/mods` that says it is installed
    pub dll: &'static str,
    pub name: &'static str,
    pub sub: &'static str,
    pub options: &'static [KnownOpt],
    /// An EML mod (eml-mods/): its folder under `<game>/mods`. EML does not read shroudtopia.json, so its on/off is
    /// `enabled=` in that folder's config.txt, which the mod reads when the game starts (the window says it needs a
    /// restart).
    pub eml: Option<&'static str>,
}

/// The mods this repository ships, with the names the window shows (the prototype's). The basics mod reads
/// `_no_resource_cost` (with the underscore) on this build, so that is the key the toggle writes.
pub const CATALOG: &[KnownMod] = &[
    KnownMod {
        key: "basics",
        dll: "basics_mod.dll",
        name: "Básicos",
        sub: "Stamina, queda e custo de recursos",
        options: &[
            KnownOpt { key: "no_stamina_loss", label: "Sem gasto de stamina", kind: OptKind::Bool },
            KnownOpt { key: "no_fall_damage", label: "Sem dano de queda", kind: OptKind::Bool },
            KnownOpt { key: "_no_resource_cost", label: "Construir e usar itens sem gastar", kind: OptKind::Bool },
            KnownOpt { key: "hide_stamina_gauge", label: "Esconder a barra de stamina", kind: OptKind::Bool },
        ],
        eml: None,
    },
    KnownMod { key: "Flight Mod", dll: "flight_mod.dll", name: "Voo", sub: "Planar sem perder altura", options: &[], eml: None },
    KnownMod {
        key: "FirstPersonView",
        dll: "first_person_view.dll",
        name: "Primeira pessoa",
        sub: "Câmera nos olhos do personagem",
        options: &[KnownOpt { key: "camera_distance", label: "Distância da câmera", kind: OptKind::Number { default: -0.15, min: -0.5, max: 0.5, step: 0.05 } }],
        eml: None,
    },
    // third-party/Enshrouded-minimap; its other options (position, keys, style...) show by their key
    KnownMod {
        key: "minimap_mod",
        dll: "minimap_mod/minimap_mod.dll",
        name: "Minimapa",
        sub: "Mapa no canto da tela, relógio e clima (F10 mostra e esconde)",
        options: &[
            KnownOpt { key: "show_clock", label: "Relógio", kind: OptKind::Bool },
            KnownOpt { key: "show_weather", label: "Clima", kind: OptKind::Bool },
            KnownOpt { key: "show_fog_of_war", label: "Névoa onde ainda não explorou", kind: OptKind::Bool },
            KnownOpt { key: "show_other_players", label: "Outros jogadores", kind: OptKind::Bool },
            KnownOpt { key: "show_death_markers", label: "Onde você morreu", kind: OptKind::Bool },
            KnownOpt { key: "scale_percent", label: "Tamanho (%)", kind: OptKind::Number { default: 100.0, min: 50.0, max: 200.0, step: 25.0 } },
        ],
        eml: None,
    },
    KnownMod {
        key: "max-stack",
        dll: "max-stack/mod.json",
        name: "Max Stack",
        sub: "Itens empilham 5× mais, até 999",
        options: &[],
        eml: Some("max-stack"),
    },
    KnownMod {
        key: "twitch-drops",
        dll: "twitch-drops/mod.json",
        name: "Drops da Twitch",
        sub: "Quem tem um drop resgata todos; usam sem vincular a Twitch",
        options: &[],
        eml: Some("twitch-drops"),
    },
];

/// `enabled=` of an EML mod's config.txt; no file (or no line) is on, as the mod reads it.
fn eml_enabled(mods_dir: &Path, folder: &str) -> bool {
    let text = fs::read_to_string(mods_dir.join(folder).join(EML_CONFIG)).unwrap_or_default();
    !text.lines().filter_map(|l| l.trim().strip_prefix("enabled")).filter_map(|r| r.trim_start().strip_prefix('=')).any(|v| v.trim().eq_ignore_ascii_case("false"))
}

const EML_CONFIG: &str = "config.txt";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OptValue {
    Bool { value: bool },
    Number { value: f64, min: f64, max: f64, step: f64 },
    /// A value of another type (a string, a list...): shown, not edited.
    Other { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptView {
    pub key: String,
    pub label: String,
    #[serde(flatten)]
    pub value: OptValue,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModView {
    pub key: String,
    pub name: String,
    /// "Stamina, queda e custo de recursos · 4 opções"
    pub sub: String,
    pub active: bool,
    pub options: Vec<OptView>,
    /// a change applies the next time the game starts
    pub restart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModsError {
    /// The file is not there yet (the loader writes it on its first start).
    Missing,
    /// It is there and is not the JSON object the loader writes.
    Invalid(String),
    Io(String),
}

pub fn config_path(game: &Path) -> PathBuf {
    game.join(CONFIG_FILE)
}

fn known(key: &str) -> Option<&'static KnownMod> {
    CATALOG.iter().find(|m| m.key == key)
}

fn read(path: &Path) -> Result<Map<String, Value>, ModsError> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ModsError::Missing),
        Err(e) => return Err(ModsError::Io(e.to_string())),
    };
    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(ModsError::Invalid("not an object".into())),
        Err(e) => Err(ModsError::Invalid(e.to_string())),
    }
}

/// The loader's default config (shroudtopia/dllmain.cpp `defaultConfig`), for a toggle made before it ever ran.
fn default_config() -> Map<String, Value> {
    let Value::Object(map) = json!({ "active": true, "bootDelay": 3000, "updateDelay": 500, "enableLogging": true, "logLevel": "INFO", "mods": {} }) else { unreachable!() };
    map
}

fn write(path: &Path, root: &Map<String, Value>) -> Result<(), ModsError> {
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b"    "));
    serde::Serialize::serialize(root, &mut ser).map_err(|e| ModsError::Io(e.to_string()))?;
    let tmp = path.with_extension("json.shroudtopia-tmp");
    fs::write(&tmp, &out).map_err(|e| ModsError::Io(e.to_string()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        ModsError::Io(e.to_string())
    })
}

fn opt_count(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => " · 1 opção".into(),
        n => format!(" · {n} opções"),
    }
}

fn view_value(kind: Option<OptKind>, value: Option<&Value>) -> OptValue {
    match (kind, value) {
        (Some(OptKind::Bool), v) => OptValue::Bool { value: v.and_then(Value::as_bool).unwrap_or(false) },
        (Some(OptKind::Number { default, min, max, step }), v) => OptValue::Number { value: v.and_then(Value::as_f64).unwrap_or(default), min, max, step },
        (None, Some(Value::Bool(b))) => OptValue::Bool { value: *b },
        (None, Some(Value::Number(n))) => OptValue::Number { value: n.as_f64().unwrap_or(0.0), min: f64::MIN, max: f64::MAX, step: if n.is_f64() { 0.05 } else { 1.0 } },
        (None, v) => OptValue::Other { text: v.map(|v| v.to_string()).unwrap_or_default() },
    }
}

/// The mods as the window lists them: those in the file (catalogue ones in the catalogue's order, then the others in
/// the file's), plus catalogue mods whose DLL is in `<game>/mods` and that the loader has not written yet.
pub fn view(path: &Path, mods_dir: Option<&Path>) -> Result<Vec<ModView>, ModsError> {
    let root = match read(path) {
        Ok(root) => root,
        Err(ModsError::Missing) => Map::new(),
        Err(e) => return Err(e),
    };
    let empty = Map::new();
    let mods = root.get("mods").and_then(Value::as_object).unwrap_or(&empty);
    let mut out = Vec::new();
    for k in CATALOG {
        if let Some(folder) = k.eml {
            // listed only when it is installed; its state is in its own folder, not in shroudtopia.json
            if let Some(dir) = mods_dir.filter(|d| d.join(k.dll).is_file()) {
                let mut v = mod_view(k.key, &empty, Some(k));
                v.active = eml_enabled(dir, folder);
                out.push(v);
            }
            continue;
        }
        let entry = mods.get(k.key).and_then(Value::as_object);
        let installed = mods_dir.is_some_and(|d| d.join(k.dll).is_file());
        if entry.is_none() && !installed {
            continue;
        }
        // what the file does not say, the mod's own defaults do (as the loader and the mod read it)
        let mut merged = mods_dir.map(|d| manifest_defaults(d, k)).unwrap_or_default();
        merged.extend(entry.unwrap_or(&empty).iter().map(|(k, v)| (k.clone(), v.clone())));
        out.push(mod_view(k.key, &merged, Some(k)));
    }
    for (key, entry) in mods {
        if known(key).is_none() {
            out.push(mod_view(key, entry.as_object().unwrap_or(&empty), None));
        }
    }
    Ok(out)
}

/// `shroudtopia.default` of the mod.json beside a folder mod's DLL (`minimap_mod/mod.json`): the values the loader
/// writes the first time it loads the mod, and only when shroudtopia.json has no entry for it yet.
fn manifest_defaults(mods_dir: &Path, k: &KnownMod) -> Map<String, Value> {
    let Some((folder, _)) = k.dll.split_once('/') else { return Map::new() };
    let text = fs::read_to_string(mods_dir.join(folder).join("mod.json")).unwrap_or_default();
    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Object(mut m)) => match m.remove("shroudtopia").and_then(|mut s| s.get_mut("default").map(Value::take)) {
            Some(Value::Object(d)) => d,
            _ => Map::new(),
        },
        _ => Map::new(),
    }
}

fn mod_view(key: &str, entry: &Map<String, Value>, known: Option<&KnownMod>) -> ModView {
    let mut options = Vec::new();
    if let Some(k) = known {
        for o in k.options {
            options.push(OptView { key: o.key.into(), label: o.label.into(), value: view_value(Some(o.kind), entry.get(o.key)) });
        }
    }
    for (okey, value) in entry {
        if okey == "active" || known.is_some_and(|k| k.options.iter().any(|o| o.key == okey)) {
            continue;
        }
        options.push(OptView { key: okey.clone(), label: okey.clone(), value: view_value(None, Some(value)) });
    }
    let (name, sub) = known.map(|k| (k.name.to_string(), k.sub.to_string())).unwrap_or_else(|| (key.to_string(), String::new()));
    let sub = format!("{sub}{}", opt_count(options.len())).trim_start_matches(" · ").to_string();
    ModView { key: key.into(), name, sub, active: entry.get("active").and_then(Value::as_bool).unwrap_or(false), options, restart: known.is_some_and(|k| k.eml.is_some()) }
}

/// Sets `mods.<mod_key>.<opt_key>` (`active` for the mod's own toggle) and writes the file.
pub fn set(path: &Path, mod_key: &str, opt_key: &str, value: Value) -> Result<(), ModsError> {
    if let Some(folder) = known(mod_key).and_then(|k| k.eml) {
        let (Some(on), "active") = (value.as_bool(), opt_key) else { return Err(ModsError::Invalid(format!("{mod_key} só liga e desliga"))) };
        let dir = path.parent().unwrap_or(Path::new(".")).join("mods").join(folder);
        if !dir.is_dir() {
            return Err(ModsError::Missing);
        }
        return fs::write(dir.join(EML_CONFIG), format!("enabled={on}\n")).map_err(|e| ModsError::Io(e.to_string()));
    }
    let mut root = match read(path) {
        Ok(root) => root,
        Err(ModsError::Missing) => default_config(),
        Err(e) => return Err(e),
    };
    let value = match (&value, known(mod_key).and_then(|k| k.options.iter().find(|o| o.key == opt_key)).map(|o| o.kind)) {
        (Value::Number(n), Some(OptKind::Number { min, max, step, .. })) => {
            let v = n.as_f64().unwrap_or(0.0).clamp(min, max);
            if step.fract() == 0.0 {
                // whole steps are written as integers (the minimap's scale_percent is 100, not 100.0)
                json!(((v / step).round() * step) as i64)
            } else {
                let digits = (1.0 / step).log10().ceil().max(0.0) as i32;
                let scale = 10f64.powi(digits);
                json!((v * scale).round() / scale)
            }
        }
        (Value::Number(n), _) if n.is_f64() => json!((n.as_f64().unwrap_or(0.0) * 1e6).round() / 1e6),
        _ => value,
    };
    let mods = root.entry("mods").or_insert_with(|| json!({}));
    if !mods.is_object() {
        *mods = json!({});
    }
    let entry = mods.as_object_mut().unwrap().entry(mod_key.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let entry = entry.as_object_mut().unwrap();
    // A toggle made before the loader wrote the mod's defaults creates its entry, and the loader then never writes
    // them: fill in every default the entry lacks, so the file says what the window showed.
    if let (Some(k), Some(game)) = (known(mod_key), path.parent()) {
        for (dk, dv) in manifest_defaults(&game.join("mods"), k) {
            entry.entry(dk).or_insert(dv);
        }
    }
    entry.insert(opt_key.to_string(), value);
    write(path, &root)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = include_str!("../fixtures/shroudtopia.json");

    fn copy() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE);
        fs::write(&path, REAL).unwrap();
        (dir, path)
    }

    #[test]
    fn the_real_file_reads_as_the_prototype_shows_it() {
        let (_dir, path) = copy();
        let mods = view(&path, None).unwrap();
        assert_eq!(mods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Básicos", "Voo", "Primeira pessoa"]);
        let basics = &mods[0];
        assert_eq!(basics.sub, "Stamina, queda e custo de recursos · 4 opções");
        assert!(basics.active);
        let values: Vec<(&str, &OptValue)> = basics.options.iter().map(|o| (o.label.as_str(), &o.value)).collect();
        assert_eq!(
            values,
            [
                ("Sem gasto de stamina", &OptValue::Bool { value: true }),
                ("Sem dano de queda", &OptValue::Bool { value: true }),
                ("Construir e usar itens sem gastar", &OptValue::Bool { value: false }),
                ("Esconder a barra de stamina", &OptValue::Bool { value: true }),
            ]
        );
        assert_eq!(mods[1].sub, "Planar sem perder altura");
        assert!(mods[1].options.is_empty() && mods[1].active);
        assert_eq!(mods[2].sub, "Câmera nos olhos do personagem · 1 opção");
        assert!(!mods[2].active);
        assert_eq!(mods[2].options[0].value, OptValue::Number { value: -0.15, min: -0.5, max: 0.5, step: 0.05 });
    }

    #[test]
    fn a_toggle_changes_one_value_and_keeps_everything_else() {
        let (_dir, path) = copy();
        set(&path, "FirstPersonView", "active", json!(true)).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        // the same text but the one value: order, indent and the keys the app does not know are kept
        assert_eq!(after.replace("\r\n", "\n").trim_end(), REAL.replace("\r\n", "\n").trim_end().replacen("\"active\": false", "\"active\": true", 1));
        set(&path, "FirstPersonView", "camera_distance", json!(-0.15 + 0.05)).unwrap();
        assert_eq!(view(&path, None).unwrap()[2].options[0].value, OptValue::Number { value: -0.1, min: -0.5, max: 0.5, step: 0.05 });
        set(&path, "FirstPersonView", "camera_distance", json!(9.0)).unwrap();
        assert!(matches!(view(&path, None).unwrap()[2].options[0].value, OptValue::Number { value, .. } if value == 0.5), "clamped");
        set(&path, "basics", "_no_resource_cost", json!(true)).unwrap();
        let root: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["mods"]["basics"]["_no_resource_cost"], true);
        assert_eq!(root["bootDelay"], 3000);
        assert!(!path.with_extension("json.shroudtopia-tmp").exists());
    }

    #[test]
    fn unknown_mods_and_options_show_by_their_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE);
        fs::write(&path, r#"{"mods":{"whitelist":{"active":true,"players":["a"],"limit":3},"basics":{"active":false,"_inf_item_use":true}}}"#).unwrap();
        let mods = view(&path, None).unwrap();
        assert_eq!(mods[0].key, "basics");
        assert_eq!(mods[0].options.len(), 5);
        assert_eq!(mods[0].options[4].label, "_inf_item_use");
        assert_eq!(mods[1].name, "whitelist");
        assert_eq!(mods[1].sub, "2 opções");
        assert_eq!(mods[1].options[0].value, OptValue::Other { text: "[\"a\"]".into() });
        assert!(matches!(mods[1].options[1].value, OptValue::Number { value, step, .. } if value == 3.0 && step == 1.0));
    }

    #[test]
    fn a_missing_file_lists_installed_mods_and_is_written_on_the_first_toggle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE);
        fs::create_dir_all(dir.path().join("mods")).unwrap();
        fs::write(dir.path().join("mods/flight_mod.dll"), b"MZ").unwrap();
        let mods = view(&path, Some(&dir.path().join("mods"))).unwrap();
        assert_eq!(mods.len(), 1);
        assert!(!mods[0].active);
        set(&path, "Flight Mod", "active", json!(true)).unwrap();
        let root: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["mods"]["Flight Mod"]["active"], true);
        assert_eq!(root["updateDelay"], 500);
        fs::write(&path, "{ not json").unwrap();
        assert!(matches!(view(&path, None), Err(ModsError::Invalid(_))));
        assert!(matches!(set(&path, "basics", "active", json!(true)), Err(ModsError::Invalid(_))), "a broken file is not overwritten");
    }

    #[test]
    fn a_folder_mod_shows_and_writes_its_manifest_defaults() {
        let (dir, path) = copy();
        let mods_dir = dir.path().join("mods");
        fs::create_dir_all(mods_dir.join("minimap_mod")).unwrap();
        fs::write(mods_dir.join("minimap_mod/minimap_mod.dll"), b"MZ").unwrap();
        fs::write(mods_dir.join("minimap_mod/mod.json"), r#"{"id":"minimap_mod","shroudtopia":{"default":{"active":true,"show_clock":true,"show_weather":true,"position":"bottom-right","scale_percent":100}}}"#).unwrap();
        let minimap = |p: &Path| view(p, Some(&mods_dir)).unwrap().into_iter().find(|m| m.key == "minimap_mod").unwrap();
        let m = minimap(&path);
        assert!(m.active, "not in the file yet: the manifest's active");
        let value = |m: &ModView, key: &str| m.options.iter().find(|o| o.key == key).unwrap().value.clone();
        assert_eq!(value(&m, "show_clock"), OptValue::Bool { value: true });
        assert_eq!(value(&m, "position"), OptValue::Other { text: "\"bottom-right\"".into() });
        // the first toggle writes the defaults with it; the file's own values win over them afterwards
        set(&path, "minimap_mod", "show_weather", json!(false)).unwrap();
        let root: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["mods"]["minimap_mod"], json!({"active":true,"show_clock":true,"show_weather":false,"position":"bottom-right","scale_percent":100}));
        assert_eq!(value(&minimap(&path), "show_weather"), OptValue::Bool { value: false });
    }

    #[test]
    fn an_eml_mod_toggles_in_its_own_folder_and_needs_a_restart() {
        let (dir, path) = copy();
        let mods_dir = dir.path().join("mods");
        let listed = |p: &Path| view(p, Some(&mods_dir)).unwrap().into_iter().find(|m| m.key == "max-stack");
        assert!(listed(&path).is_none(), "not installed, not listed");
        fs::create_dir_all(mods_dir.join("max-stack")).unwrap();
        fs::write(mods_dir.join("max-stack/mod.json"), "{}").unwrap();
        let m = listed(&path).unwrap();
        assert_eq!((m.name.as_str(), m.active, m.restart), ("Max Stack", true, true), "no config.txt is on");
        let before = fs::read_to_string(&path).unwrap();
        set(&path, "max-stack", "active", json!(false)).unwrap();
        assert_eq!(fs::read_to_string(mods_dir.join("max-stack/config.txt")).unwrap(), "enabled=false\n");
        assert!(!listed(&path).unwrap().active);
        assert_eq!(fs::read_to_string(&path).unwrap(), before, "shroudtopia.json is not touched");
        set(&path, "max-stack", "active", json!(true)).unwrap();
        assert!(listed(&path).unwrap().active);
        assert!(!view(&path, Some(&mods_dir)).unwrap()[0].restart);
    }
}
