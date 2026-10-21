//! What the window shows, in the prototype's words (pt-BR), built from the engine's real state. The Svelte side only
//! lays it out.

use chrono::{DateTime, FixedOffset, Utc};
use serde::Serialize;

use crate::contract::Version;
use crate::engine::Engine;
use crate::format as f;
use crate::saves::StoreKind;
use crate::sync_state::{Holder, SyncState, Transfer};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub label: String,
    /// play | join | none
    pub kind: String,
    pub icon: Option<String>,
    /// primary | friend | ""
    pub cls: String,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    pub text: String,
    /// info | alert
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgressView {
    pub pct: u8,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tag {
    /// here | cloud | live | friendlive | warn
    pub cls: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistRow {
    /// me | friend
    pub who: String,
    pub initial: String,
    pub name: String,
    pub when: String,
    pub size: String,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choice {
    pub initial: String,
    pub t: String,
    pub s: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choices {
    pub mine: Choice,
    pub theirs: Choice,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldView {
    pub name: String,
    /// "com rodrigao_br"
    pub aside: String,
    pub state: SyncState,
    /// ok | live | friend | busy | warn | muted
    pub tone: String,
    pub status: String,
    pub line: String,
    pub action: Option<Action>,
    pub progress: Option<ProgressView>,
    pub note: Option<Note>,
    pub choices: Option<Choices>,
    pub history: Vec<HistRow>,
    pub file: String,
    pub store: StoreKind,
    pub members: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub nick: String,
    pub initial: String,
    pub me: bool,
    pub sub: String,
    /// live | ok | friend | ""
    pub dot: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// setup (no shared world yet) | main
    pub phase: String,
    pub player: Option<Person>,
    pub game_running: bool,
    pub online: bool,
    pub world: Option<WorldView>,
    pub people: Vec<Person>,
    pub error: Option<String>,
    pub login: LoginView,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginView {
    /// the browser is open on the Steam login ("Termine o login no navegador")
    pub waiting: bool,
    /// the account the browser logged into, when it is not this PC's Steam account
    pub mismatch: Option<String>,
    /// this PC's Steam account (`loginusers.vdf`)
    pub steam_nick: Option<String>,
    pub error: Option<String>,
}

fn action(label: &str, kind: &str, icon: Option<&str>, cls: &str, disabled: bool) -> Option<Action> {
    Some(Action { label: label.into(), kind: kind.into(), icon: icon.map(Into::into), cls: cls.into(), disabled })
}

fn tag(cls: &str) -> Tag {
    let text = match cls {
        "here" => "neste PC",
        "cloud" => "no servidor",
        "live" | "friendlive" => "jogando agora",
        _ => "versão separada",
    };
    Tag { cls: cls.into(), text: text.into() }
}

pub fn progress_of(t: Transfer) -> Option<ProgressView> {
    match t {
        Transfer::Idle => None,
        Transfer::Up { done, total } | Transfer::Down { done, total } => Some(ProgressView { pct: f::percent(done, total), text: f::progress(done, total) }),
    }
}

/// The bytes of a transfer under way, on a snapshot made when it began.
pub fn patch_progress(s: &mut Snapshot, t: Transfer) {
    if let Some(w) = s.world.as_mut() {
        w.progress = progress_of(t);
    }
}

/// A session of the history: the versions one player uploaded in one sitting (autosaves and the final one).
struct Group<'a> {
    first: &'a Version,
    last: &'a Version,
    ids: Vec<&'a str>,
}

fn sessions(history: &[Version]) -> Vec<Group<'_>> {
    let mut out: Vec<Group> = Vec::new();
    for v in history {
        match out.last_mut() {
            Some(g) if g.last.uploaded_by == v.uploaded_by && g.last.started_at == v.started_at => {
                g.ids.push(&v.version_id);
                g.first = v;
            }
            _ => out.push(Group { first: v, last: v, ids: vec![&v.version_id] }),
        }
    }
    out
}

pub fn snapshot(e: &mut Engine) -> Snapshot {
    let now = e.rt.now;
    let off = e.rt.offset.unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    let me_id = e.me().unwrap_or("").to_string();
    let me_nick = e.cfg.player.as_ref().or(e.rt.local_player.as_ref()).map(|p| p.nick.clone()).unwrap_or_default();
    let state = if e.cfg.world.is_some() { Some(e.state()) } else { None };
    let local_changed = e.cfg.world.is_some() && e.local_changed();
    let (mine, theirs) = if state == Some(SyncState::Diverged) { e.competing() } else { (None, None) };
    let e: &Engine = e;
    let holder = e.holder();

    let nick_of = |id: &str| -> String {
        let w = e.rt.world.as_ref();
        w.and_then(|w| w.members.iter().find(|m| m.steam_id == id).map(|m| m.nick.clone()))
            .or_else(|| e.rt.presence.iter().find(|p| p.steam_id == id).map(|p| p.nick.clone()))
            .or_else(|| w.and_then(|w| w.lock.as_ref()).filter(|l| l.holder == id).map(|l| l.nick.clone()))
            .unwrap_or_else(|| "seu amigo".into())
    };
    let others: Vec<String> = e.rt.world.as_ref().map(|w| w.members.iter().filter(|m| m.steam_id != me_id).map(|m| m.nick.clone()).collect()).unwrap_or_default();
    let friend = others.first().cloned();

    let me_person = e.cfg.player.as_ref().or(e.rt.local_player.as_ref()).map(|p| Person {
        nick: p.nick.clone(),
        initial: f::initial(&p.nick),
        me: true,
        sub: if state == Some(SyncState::Me) { "hospedando".into() } else { "online".into() },
        dot: if state == Some(SyncState::Me) { "live".into() } else { "ok".into() },
    });
    let mut people: Vec<Person> = me_person.clone().into_iter().collect();
    if let Some(w) = e.rt.world.as_ref() {
        for m in w.members.iter().filter(|m| m.steam_id != me_id) {
            let p = e.rt.presence.iter().find(|p| p.steam_id == m.steam_id);
            let hosting = holder == Holder::Other && w.lock.as_ref().is_some_and(|l| l.holder == m.steam_id);
            let (sub, dot) = if !e.rt.online {
                ("—", "")
            } else if hosting {
                ("hospedando", "friend")
            } else if p.is_some_and(|p| p.in_game) {
                ("jogando", "ok")
            } else if p.is_some_and(|p| p.app_online) {
                ("online", "ok")
            } else {
                ("offline", "")
            };
            people.push(Person { nick: m.nick.clone(), initial: f::initial(&m.nick), me: false, sub: sub.into(), dot: dot.into() });
        }
    }

    let world = match (state.filter(|_| e.logged_in()), e.cfg.world.clone()) {
        (Some(state), Some(link)) => {
            let w = e.rt.world.clone();
            let head = w.as_ref().and_then(|w| w.head.clone());
            let who = |id: &str| if id == me_id { "Você".to_string() } else { nick_of(id) };
            let game = e.rt.game_running;
            let open = || action("Enshrouded aberto", "none", None, "", true);
            let play = |disabled: bool| if game { open() } else { action("Jogar", "play", Some("play"), "primary", disabled) };
            let lock = e.foreign_lock().cloned();
            let session = link.session.clone();
            let (tone, status, line, act, note): (&str, String, String, Option<Action>, Option<Note>) = match state {
                SyncState::Sync => {
                    let line = match &head {
                        Some(h) => {
                            let end = h.ended_at.unwrap_or(h.created_at);
                            format!("{} jogou por último · {} · {} de sessão", who(&h.uploaded_by), f::day_time(end, now, off), f::duration(h.started_at, end))
                        }
                        None => "Nenhuma sessão enviada ainda".into(),
                    };
                    ("ok", "Em dia".into(), line, play(false), None)
                }
                SyncState::Me => {
                    let start = session.as_ref().map(|s| s.started_at).unwrap_or(now);
                    let auto = match session.as_ref().and_then(|s| s.last_autosave_at) {
                        Some(t) => format!("último autosave enviado às {}", f::hm(t, off)),
                        None => "nenhum autosave enviado ainda".into(),
                    };
                    ("live", "Você está hospedando".into(), format!("{} · {auto}", f::since(start, now, off)), open(), None)
                }
                SyncState::Friend => {
                    let (nick, since) = lock.as_ref().map(|l| (l.nick.clone(), l.since)).unwrap_or_else(|| (friend.clone().unwrap_or_default(), now));
                    let act = if game { open() } else { action("Entrar pelo Steam", "join", Some("join"), "friend", false) };
                    (
                        "friend",
                        format!("{nick} está hospedando"),
                        format!("{} · o mundo fica com ele até fechar o jogo", f::since(since, now, off)),
                        act,
                        Some(Note { text: "Abrir o mundo daqui fica travado enquanto ele joga, para ninguém perder progresso.".into(), icon: "info".into() }),
                    )
                }
                SyncState::Behind => {
                    let h = head.clone();
                    let (status, line) = match &h {
                        Some(h) if h.uploaded_by == me_id => ("Baixando sua sessão".to_string(), format!("Você jogou {}", f::range(h.started_at, h.ended_at, now, off))),
                        Some(h) => (format!("Baixando a sessão do {}", who(&h.uploaded_by)), format!("{} jogou {}", who(&h.uploaded_by), f::range(h.started_at, h.ended_at, now, off))),
                        None => ("Baixando".into(), String::new()),
                    };
                    let line = if game && e.rt.transfer == Transfer::Idle { format!("{line} · baixa quando você fechar o jogo") } else { line };
                    ("busy", status, line, play(true), None)
                }
                SyncState::Up => {
                    let p = link.pending.clone();
                    let start = p.as_ref().map(|p| p.started_at).unwrap_or(now);
                    let end = p.as_ref().and_then(|p| p.ended_at).or(Some(now));
                    ("busy", "Enviando sua sessão".into(), format!("Você jogou {}", f::range(start, end, now, off)), play(true), None)
                }
                SyncState::Diverged => ("warn", "Vocês jogaram separados".into(), "Os dois abriram o mundo sem o outro saber. Escolha qual segue.".into(), None, None),
                SyncState::Offline => (
                    "muted",
                    "Sem conexão com o servidor".into(),
                    "Dá para jogar normal. Sua sessão sobe quando a conexão voltar.".into(),
                    play(false),
                    friend.as_ref().map(|fr| Note { text: format!("Se o {fr} jogar nesse meio-tempo, vocês vão ter que escolher qual versão segue."), icon: "alert".into() }),
                ),
            };

            // the history, one row per session, newest first
            let base = link.base.clone();
            let head_id = head.as_ref().map(|h| h.version_id.clone());
            let competing: Vec<String> = [mine.as_ref(), theirs.as_ref()].into_iter().flatten().map(|v| v.version_id.clone()).collect();
            let mut rows: Vec<HistRow> = Vec::new();
            let row = |id: &str, when: String, size: String, tags: Vec<Tag>| HistRow {
                who: if id == me_id { "me".into() } else { "friend".into() },
                initial: f::initial(&if id == me_id { me_nick.clone() } else { nick_of(id) }),
                name: who(id),
                when,
                size,
                tags,
            };
            let history = w.as_ref().map(|w| w.history.clone()).unwrap_or_default();
            let groups = sessions(&history);
            let mut skip_first = false;
            match state {
                SyncState::Me => {
                    let start = session.as_ref().map(|s| s.started_at).unwrap_or(now);
                    let size = e.stores.get(link.store).and_then(|s| s.stat(link.slot)).map(|s| f::size(s.size)).unwrap_or_else(|| "—".into());
                    skip_first = groups.first().is_some_and(|g| g.last.uploaded_by == me_id && g.last.ended_at.is_none() && g.last.started_at == start);
                    rows.push(row(&me_id, f::range(start, None, now, off), size, vec![tag("live")]));
                }
                SyncState::Friend => {
                    if let Some(l) = &lock {
                        let live = groups.first().filter(|g| g.last.uploaded_by == l.holder && g.last.ended_at.is_none());
                        skip_first = live.is_some();
                        let size = live.map(|g| f::size(g.last.size)).unwrap_or_else(|| "—".into());
                        rows.push(row(&l.holder, f::range(live.map(|g| g.first.started_at).unwrap_or(l.since), None, now, off), size, vec![tag("friendlive")]));
                    }
                }
                SyncState::Up => {
                    if let Some(p) = &link.pending {
                        rows.push(row(&me_id, f::range(p.started_at, p.ended_at, now, off), f::size(p.size), vec![tag("here")]));
                    }
                }
                _ => {}
            }
            for (i, g) in groups.iter().enumerate() {
                if i == 0 && skip_first {
                    continue;
                }
                let mut tags = Vec::new();
                if !local_changed && g.ids.iter().any(|id| Some(*id) == base.as_deref()) && state != SyncState::Up {
                    tags.push(tag("here"));
                }
                if g.ids.iter().any(|id| Some(*id) == head_id.as_deref()) {
                    tags.push(tag("cloud"));
                }
                if g.ids.iter().any(|id| competing.iter().any(|c| c == id)) {
                    if mine.as_ref().is_some_and(|m| g.ids.contains(&m.version_id.as_str())) && !tags.iter().any(|t| t.cls == "here") && !local_changed {
                        tags.insert(0, tag("here"));
                    }
                    tags.push(tag("warn"));
                }
                rows.push(row(&g.last.uploaded_by, f::range(g.first.started_at, g.last.ended_at, now, off), f::size(g.last.size), tags));
            }
            rows.truncate(5);

            let choices = (state == SyncState::Diverged).then(|| {
                let fr = theirs.as_ref().map(|v| who(&v.uploaded_by)).or(friend.clone()).unwrap_or_else(|| "seu amigo".into());
                let mine_s = mine.as_ref().map(|v| f::range(v.started_at, v.ended_at, now, off)).unwrap_or_else(|| "a cópia deste PC".into());
                let theirs_s = theirs.as_ref().map(|v| f::range(v.started_at, v.ended_at, now, off)).unwrap_or_default();
                Choices {
                    mine: Choice { initial: f::initial(&me_nick), t: "Seguir com a sua".into(), s: format!("{mine_s} · neste PC") },
                    theirs: Choice { initial: f::initial(&fr), t: format!("Seguir com a do {fr}"), s: format!("{theirs_s} · no servidor") },
                }
            });
            let aside = if others.is_empty() { "só você por enquanto".to_string() } else { format!("com {}", join_names(&others)) };
            Some(WorldView {
                name: link.name.clone(),
                aside,
                state,
                tone: tone.into(),
                status,
                line,
                action: act,
                progress: progress_of(e.rt.transfer),
                note,
                choices,
                history: rows,
                file: crate::steam::slot_file(link.slot),
                store: link.store,
                members: w.as_ref().map(|w| w.members.len()).unwrap_or(1),
            })
        }
        _ => None,
    };

    Snapshot {
        phase: if !e.logged_in() { "login".into() } else if e.cfg.world.is_some() { "main".into() } else { "setup".into() },
        player: me_person,
        game_running: e.rt.game_running,
        online: e.rt.online,
        world,
        people,
        error: e.rt.error.clone(),
        login: LoginView {
            waiting: e.rt.login.is_some(),
            mismatch: e.rt.mismatch.as_ref().map(|d| d.player.nick.clone()),
            steam_nick: e.rt.local_player.as_ref().map(|p| p.nick.clone()),
            error: e.rt.login_error.clone(),
        },
    }
}

fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} e {last}", init.join(", ")),
    }
}

/// For the tray's tooltip: "Shroudtopia: Em dia".
pub fn tray_text(s: &Snapshot) -> String {
    match &s.world {
        Some(w) => format!("Shroudtopia: {}", w.status),
        None => "Shroudtopia".into(),
    }
}

#[allow(dead_code)]
fn _unused(_: DateTime<Utc>) {}
