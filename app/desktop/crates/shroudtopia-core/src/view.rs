//! What the window shows, in the prototype's words (pt-BR), built from the engine's real state. The Svelte side only
//! lays it out.

use chrono::{DateTime, FixedOffset, Utc};
use serde::Serialize;

use crate::contract::{Player, Version};
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
    pub id: String,
    pub name: String,
    /// "com rodrigao_br", under the status
    pub with: String,
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
    /// When the world last changed on the server (its newest version, or this PC's last autosave while hosting):
    /// the card's "atualizado há 5 min"
    pub updated_at: Option<DateTime<Utc>>,
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
    /// Engine::syncing: the updater waits for it to be false before it installs
    pub syncing: bool,
    pub online: bool,
    /// one card per shared world, in the order they were added
    pub worlds: Vec<WorldView>,
    /// everyone of every shared world, once
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

/// The bytes of a transfer under way, on a snapshot made when it began (the card of the world it is for).
pub fn patch_progress(s: &mut Snapshot, world_id: Option<&str>, t: Transfer) {
    for w in s.worlds.iter_mut().filter(|w| Some(w.id.as_str()) == world_id) {
        w.progress = progress_of(t);
    }
}

/// One session of a world's history: the versions one player uploaded in one sitting (the autosaves and the final
/// upload), newest first in `ids`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGroup {
    pub uploaded_by: String,
    pub started_at: DateTime<Utc>,
    /// the session's end: the last `endedAt` it has, else (a session cut short) when its newest upload arrived;
    /// None while it runs (`live`)
    pub ended_at: Option<DateTime<Utc>>,
    /// still running: no `endedAt` yet and its player holds the lease now
    pub live: bool,
    /// the newest upload's size
    pub size: u64,
    pub ids: Vec<String>,
}

/// The history (newest first, as the server sends it) grouped by session: consecutive versions with the same
/// uploader and the same `startedAt` (to the second) are one session. `live_holder` is who holds the world's lease
/// now: that player's newest session without an end shows as running ("desde 19:00").
pub fn group_sessions(history: &[Version], live_holder: Option<&str>) -> Vec<SessionGroup> {
    let mut groups: Vec<(Vec<&Version>,)> = Vec::new();
    for v in history {
        match groups.last_mut() {
            Some((g,)) if g[0].uploaded_by == v.uploaded_by && g[0].started_at.timestamp() == v.started_at.timestamp() => g.push(v),
            _ => groups.push((vec![v],)),
        }
    }
    let mut seen_live = false;
    groups
        .into_iter()
        .map(|(g,)| {
            let newest = g[0];
            let ended = g.iter().filter_map(|v| v.ended_at).max();
            let live = ended.is_none() && !seen_live && live_holder == Some(newest.uploaded_by.as_str());
            if newest.uploaded_by.as_str() == live_holder.unwrap_or("") {
                seen_live = true;
            }
            SessionGroup {
                uploaded_by: newest.uploaded_by.clone(),
                started_at: g.iter().map(|v| v.started_at).min().unwrap_or(newest.started_at),
                ended_at: if live { None } else { ended.or(Some(newest.created_at)) },
                live,
                size: newest.size,
                ids: g.iter().map(|v| v.version_id.clone()).collect(),
            }
        })
        .collect()
}

/// The history rows a card shows.
pub const HISTORY_ROWS: usize = 3;

pub fn snapshot(e: &mut Engine) -> Snapshot {
    let off = e.rt.offset.unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    let me_id = e.me().unwrap_or("").to_string();
    let me_nick = e.cfg.player.as_ref().or(e.rt.local_player.as_ref()).map(|p| p.nick.clone()).unwrap_or_default();

    // every world's card, and who is in which world
    let mut worlds = Vec::new();
    let mut members: Vec<(Player, bool)> = Vec::new();
    let mut hosting_me = false;
    let back = e.rt.cur.clone();
    for id in e.world_ids() {
        if e.select(&id).is_err() {
            continue;
        }
        if let Some(v) = world_view(e, off) {
            hosting_me |= v.state == SyncState::Me;
            worlds.push(v);
        }
        let host = e.foreign_lock().map(|l| l.holder.clone());
        if let Some(w) = e.rt.world.as_ref() {
            for m in w.members.iter().filter(|m| m.steam_id != me_id) {
                let hosting = host.as_deref() == Some(m.steam_id.as_str());
                match members.iter_mut().find(|(p, _)| p.steam_id == m.steam_id) {
                    Some(found) => found.1 |= hosting,
                    None => members.push((m.clone(), hosting)),
                }
            }
        }
    }
    if let Some(b) = back {
        let _ = e.select(&b);
    }

    let me_person = e.cfg.player.as_ref().or(e.rt.local_player.as_ref()).map(|p| Person {
        nick: p.nick.clone(),
        initial: f::initial(&p.nick),
        me: true,
        sub: if hosting_me { "hospedando".into() } else { "online".into() },
        dot: if hosting_me { "live".into() } else { "ok".into() },
    });
    let mut people: Vec<Person> = me_person.clone().into_iter().collect();
    for (m, hosting) in members {
        let p = e.rt.presence.iter().find(|p| p.steam_id == m.steam_id);
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
    let _ = me_nick;

    Snapshot {
        phase: if !e.logged_in() {
            "login".into()
        } else if worlds.is_empty() {
            "setup".into()
        } else {
            "main".into()
        },
        player: me_person,
        game_running: e.rt.game_running,
        syncing: e.syncing(),
        online: e.rt.online,
        worlds,
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

/// The card of the selected world.
fn world_view(e: &mut Engine, off: FixedOffset) -> Option<WorldView> {
    if !e.logged_in() {
        return None;
    }
    let link = e.cfg.world.clone()?;
    let now = e.rt.now;
    let me_id = e.me().unwrap_or("").to_string();
    let me_nick = e.cfg.player.as_ref().or(e.rt.local_player.as_ref()).map(|p| p.nick.clone()).unwrap_or_default();
    let state = e.state();
    let local_changed = e.local_changed();
    let (mine, theirs) = if state == SyncState::Diverged { e.competing() } else { (None, None) };
    let transfer = if e.rt.transfer_world == e.rt.cur { e.rt.transfer } else { Transfer::Idle };
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
            let (status, line) = match &head {
                Some(h) if h.uploaded_by == me_id => ("Baixando sua sessão".to_string(), format!("Você jogou {}", f::range(h.started_at, h.ended_at, now, off))),
                Some(h) => (format!("Baixando a sessão do {}", who(&h.uploaded_by)), format!("{} jogou {}", who(&h.uploaded_by), f::range(h.started_at, h.ended_at, now, off))),
                None => ("Baixando".into(), String::new()),
            };
            let line = if game && transfer == Transfer::Idle { format!("{line} · baixa quando você fechar o jogo") } else { line };
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
    let row = |id: &str, when: String, size: String, tags: Vec<Tag>| HistRow {
        who: if id == me_id { "me".into() } else { "friend".into() },
        initial: f::initial(&if id == me_id { me_nick.clone() } else { nick_of(id) }),
        name: who(id),
        when,
        size,
        tags,
    };
    let live_holder: Option<String> = match holder {
        Holder::Me if state == SyncState::Me => Some(me_id.clone()),
        Holder::Other => lock.as_ref().map(|l| l.holder.clone()),
        _ => None,
    };
    let history = w.as_ref().map(|w| w.history.clone()).unwrap_or_default();
    let groups = group_sessions(&history, live_holder.as_deref());
    let mut rows: Vec<HistRow> = Vec::new();
    // a session running with nothing uploaded yet still shows, first
    match (&state, groups.iter().any(|g| g.live)) {
        (SyncState::Me, false) => {
            let start = session.as_ref().map(|s| s.started_at).unwrap_or(now);
            let size = e.stores.get(link.store).and_then(|s| s.stat(link.slot)).map(|s| f::size(s.size)).unwrap_or_else(|| "—".into());
            rows.push(row(&me_id, f::range(start, None, now, off), size, vec![tag("live")]));
        }
        (SyncState::Friend, false) => {
            if let Some(l) = &lock {
                rows.push(row(&l.holder, f::range(l.since, None, now, off), "—".into(), vec![tag("friendlive")]));
            }
        }
        (SyncState::Up, _) => {
            if let Some(p) = &link.pending {
                rows.push(row(&me_id, f::range(p.started_at, p.ended_at, now, off), f::size(p.size), vec![tag("here")]));
            }
        }
        _ => {}
    }
    for g in &groups {
        let mut tags = Vec::new();
        if g.live {
            tags.push(tag(if g.uploaded_by == me_id { "live" } else { "friendlive" }));
        }
        if !g.live && !local_changed && g.ids.iter().any(|id| Some(id.as_str()) == base.as_deref()) && state != SyncState::Up {
            tags.push(tag("here"));
        }
        if !g.live && g.ids.iter().any(|id| Some(id.as_str()) == head_id.as_deref()) {
            tags.push(tag("cloud"));
        }
        if g.ids.iter().any(|id| competing.iter().any(|c| c == id)) {
            if mine.as_ref().is_some_and(|m| g.ids.contains(&m.version_id)) && !tags.iter().any(|t| t.cls == "here") && !local_changed {
                tags.insert(0, tag("here"));
            }
            tags.push(tag("warn"));
        }
        rows.push(row(&g.uploaded_by, f::range(g.started_at, g.ended_at, now, off), f::size(g.size), tags));
    }
    rows.truncate(HISTORY_ROWS);

    let choices = (state == SyncState::Diverged).then(|| {
        let fr = theirs.as_ref().map(|v| who(&v.uploaded_by)).or(friend.clone()).unwrap_or_else(|| "seu amigo".into());
        let mine_s = mine.as_ref().map(|v| f::range(v.started_at, v.ended_at, now, off)).unwrap_or_else(|| "a cópia deste PC".into());
        let theirs_s = theirs.as_ref().map(|v| f::range(v.started_at, v.ended_at, now, off)).unwrap_or_default();
        Choices {
            mine: Choice { initial: f::initial(&me_nick), t: "Seguir com a sua".into(), s: format!("{mine_s} · neste PC") },
            theirs: Choice { initial: f::initial(&fr), t: format!("Seguir com a do {fr}"), s: format!("{theirs_s} · no servidor") },
        }
    });
    let with = if others.is_empty() { "só você por enquanto".to_string() } else { format!("com {}", join_names(&others)) };
    Some(WorldView {
        id: link.world_id.clone(),
        name: link.name.clone(),
        with,
        state,
        tone: tone.into(),
        status,
        line,
        action: act,
        progress: progress_of(transfer),
        note,
        choices,
        history: rows,
        file: crate::steam::slot_file(link.slot),
        store: link.store,
        members: w.as_ref().map(|w| w.members.len()).unwrap_or(1),
        updated_at: head.as_ref().map(|h| h.created_at).max(session.as_ref().and_then(|s| s.last_autosave_at)),
    })
}

fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} e {last}", init.join(", ")),
    }
}

/// For the tray's tooltip: "Shroudtopia: Em dia" (one world), "Shroudtopia: CopyWithGabe, Em dia · Outro, ..." (more).
pub fn tray_text(s: &Snapshot) -> String {
    match s.worlds.as_slice() {
        [] => "Shroudtopia".into(),
        [w] => format!("Shroudtopia: {}", w.status),
        ws => format!("Shroudtopia: {}", ws.iter().map(|w| format!("{}, {}", w.name, w.status)).collect::<Vec<_>>().join(" · ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn br() -> FixedOffset {
        FixedOffset::west_opt(3 * 3600).unwrap()
    }
    fn at(d: u32, h: u32, m: u32, s: u32) -> DateTime<Utc> {
        br().with_ymd_and_hms(2026, 10, d, h, m, s).unwrap().with_timezone(&Utc)
    }
    fn v(id: &str, by: &str, start: DateTime<Utc>, end: Option<DateTime<Utc>>, created: DateTime<Utc>, size: u64) -> Version {
        Version { version_id: id.into(), parent: None, uploaded_by: by.into(), size, sha256: "0".repeat(64), started_at: start, ended_at: end, created_at: created }
    }

    const ME: &str = "76561198065863968";
    const FR: &str = "76561198000000001";

    /// What the server sent for "CopyWithGabe" on 2026-10-21: five autosaves of Paulo's session from 19:00, its
    /// final upload, then the friend's session of the day before (two versions), newest first. Three sessions.
    fn copy_with_gabe() -> Vec<Version> {
        let s = at(21, 19, 0, 4) + chrono::Duration::milliseconds(512);
        let fr = at(20, 21, 48, 0);
        vec![
            v("v8", ME, s, Some(at(21, 19, 42, 0)), at(21, 19, 42, 6), 1_887_437),
            v("v7", ME, s, None, at(21, 19, 35, 0), 1_880_000),
            v("v6", ME, s, None, at(21, 19, 27, 0), 1_870_000),
            v("v5", ME, s, None, at(21, 19, 18, 0), 1_860_000),
            v("v4", ME, s, None, at(21, 19, 11, 0), 1_850_000),
            v("v3", ME, s, None, at(21, 19, 5, 0), 1_840_000),
            v("v2", FR, fr, Some(at(20, 23, 40, 0)), at(20, 23, 40, 5), 1_797_049),
            v("v1", FR, fr, None, at(20, 22, 30, 0), 1_790_000),
            v("v0", ME, at(18, 20, 30, 0), Some(at(18, 23, 58, 0)), at(18, 23, 58, 3), 1_700_000),
        ]
    }

    #[test]
    fn autosaves_of_one_session_are_one_row() {
        let g = group_sessions(&copy_with_gabe(), None);
        assert_eq!(g.len(), 3);
        assert_eq!(g[0].ids, ["v8", "v7", "v6", "v5", "v4", "v3"]);
        assert_eq!(g[0].uploaded_by, ME);
        assert_eq!(f::range(g[0].started_at, g[0].ended_at, at(21, 22, 0, 0), br()), "hoje, 19:00 – 19:42");
        assert_eq!(g[0].size, 1_887_437, "the newest upload's size");
        assert!(!g[0].live);
        assert_eq!(g[1].ids, ["v2", "v1"]);
        assert_eq!(f::range(g[1].started_at, g[1].ended_at, at(21, 22, 0, 0), br()), "ontem, 21:48 – 23:40");
        assert_eq!(g[2].ids, ["v0"]);
    }

    #[test]
    fn a_running_session_is_desde_while_its_player_holds_the_lease() {
        // the session is still going: only autosaves (no endedAt)
        let running: Vec<Version> = copy_with_gabe().into_iter().skip(1).collect();
        let g = group_sessions(&running, Some(ME));
        assert!(g[0].live);
        assert_eq!(g[0].ended_at, None);
        assert_eq!(f::range(g[0].started_at, g[0].ended_at, at(21, 19, 40, 0), br()), "hoje, desde 19:00");
        assert_eq!(g[0].ids.len(), 5);
        // the lease is gone (a crash): the session ends at its last upload
        let g = group_sessions(&running, None);
        assert!(!g[0].live);
        assert_eq!(f::range(g[0].started_at, g[0].ended_at, at(21, 22, 0, 0), br()), "hoje, 19:00 – 19:35");
        // the friend holds the lease: Paulo's unfinished session is not his running one
        let g = group_sessions(&running, Some(FR));
        assert!(!g[0].live && !g[1].live, "the friend's newest session has an end");
    }

    #[test]
    fn sessions_split_on_another_player_or_another_start() {
        let s = at(21, 19, 0, 0);
        let h = vec![
            v("c", ME, at(21, 20, 0, 0), None, at(21, 20, 5, 0), 3),
            v("b", ME, s, Some(at(21, 19, 30, 0)), at(21, 19, 30, 1), 2),
            v("a", FR, s, Some(at(21, 19, 20, 0)), at(21, 19, 20, 1), 1),
        ];
        let g = group_sessions(&h, Some(ME));
        assert_eq!(g.iter().map(|g| g.ids.len()).collect::<Vec<_>>(), [1, 1, 1]);
        assert!(g[0].live && !g[1].live);
        // the same start to the millisecond or to the second is one session
        let h = vec![v("y", ME, s + chrono::Duration::milliseconds(400), None, at(21, 19, 9, 0), 2), v("x", ME, s, None, at(21, 19, 5, 0), 1)];
        assert_eq!(group_sessions(&h, Some(ME)).len(), 1);
        assert!(group_sessions(&[], None).is_empty());
    }
}
