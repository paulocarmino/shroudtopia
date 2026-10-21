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
    pub dll: &'static str,
    pub name: &'static str,
    pub sub: &'static str,
    pub options: &'static [KnownOpt],
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
    },
    KnownMod { key: "Flight Mod", dll: "flight_mod.dll", name: "Voo", sub: "Planar sem perder altura", options: &[] },
    KnownMod {
        key: "FirstPersonView",
        dll: "first_person_view.dll",
        name: "Primeira pessoa",
        sub: "Câmera nos olhos do personagem",
        options: &[KnownOpt { key: "camera_distance", label: "Distância da câmera", kind: OptKind::Number { default: -0.15, min: -0.5, max: 0.5, step: 0.05 } }],
    },
];

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
        let entry = mods.get(k.key).and_then(Value::as_object);
        let installed = mods_dir.is_some_and(|d| d.join(k.dll).is_file());
        if entry.is_none() && !installed {
            continue;
        }
        out.push(mod_view(k.key, entry.unwrap_or(&empty), Some(k)));
    }
    for (key, entry) in mods {
        if known(key).is_none() {
            out.push(mod_view(key, entry.as_object().unwrap_or(&empty), None));
        }
    }
    Ok(out)
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
    ModView { key: key.into(), name, sub, active: entry.get("active").and_then(Value::as_bool).unwrap_or(false), options }
}

/// Sets `mods.<mod_key>.<opt_key>` (`active` for the mod's own toggle) and writes the file.
pub fn set(path: &Path, mod_key: &str, opt_key: &str, value: Value) -> Result<(), ModsError> {
    let mut root = match read(path) {
        Ok(root) => root,
        Err(ModsError::Missing) => default_config(),
        Err(e) => return Err(e),
    };
    let value = match (&value, known(mod_key).and_then(|k| k.options.iter().find(|o| o.key == opt_key)).map(|o| o.kind)) {
        (Value::Number(n), Some(OptKind::Number { min, max, step, .. })) => {
            let v = n.as_f64().unwrap_or(0.0).clamp(min, max);
            let digits = (1.0 / step).log10().ceil().max(0.0) as i32;
            let scale = 10f64.powi(digits);
            json!((v * scale).round() / scale)
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
    entry.as_object_mut().unwrap().insert(opt_key.to_string(), value);
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
}
