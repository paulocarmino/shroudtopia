//! The world card's state: one of the seven of the approved prototype, derived from what the engine knows. Pure, so
//! every state and every conflict between inputs is a row of the table test below.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncState {
    /// "Em dia"
    Sync,
    /// "Você está hospedando"
    Me,
    /// "<amigo> está hospedando"
    Friend,
    /// "Baixando a sessão do <amigo>"
    Behind,
    /// "Enviando sua sessão"
    Up,
    /// "Vocês jogaram separados"
    Diverged,
    /// "Sem conexão com o servidor"
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Holder {
    #[default]
    None,
    Me,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transfer {
    #[default]
    Idle,
    Down { done: u64, total: u64 },
    Up { done: u64, total: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Inputs {
    /// the last request to the server answered
    pub online: bool,
    pub game_running: bool,
    /// who holds the host lease (None when offline: what was known is stale)
    pub lock: Holder,
    /// the server says two versions compete (`World.diverged`)
    pub server_diverged: bool,
    /// the server has a head and the local file is not it
    pub behind: bool,
    /// the local file changed since the version it came from
    pub local_changed: bool,
    pub transfer: Transfer,
}

/// Precedence: a transfer under way > diverged > hosting (me, then a friend) > offline > behind > in sync.
pub fn derive(i: &Inputs) -> SyncState {
    match i.transfer {
        Transfer::Down { .. } => return SyncState::Behind,
        // an autosave going up while the game runs is part of hosting
        Transfer::Up { .. } if !i.game_running => return SyncState::Up,
        _ => {}
    }
    if i.online && (i.server_diverged || (i.behind && i.local_changed)) {
        return SyncState::Diverged;
    }
    if i.online && i.game_running && i.lock == Holder::Me {
        return SyncState::Me;
    }
    if i.online && i.lock == Holder::Other {
        return SyncState::Friend;
    }
    if !i.online {
        return SyncState::Offline;
    }
    if i.behind && !i.local_changed {
        return SyncState::Behind;
    }
    SyncState::Sync
}

#[cfg(test)]
mod tests {
    use super::*;
    use SyncState::*;

    fn on() -> Inputs {
        Inputs { online: true, ..Default::default() }
    }

    #[test]
    fn every_prototype_state_and_the_conflicts_between_inputs() {
        let up = Transfer::Up { done: 1, total: 2 };
        let down = Transfer::Down { done: 1, total: 2 };
        let rows: Vec<(&str, Inputs, SyncState)> = vec![
            // the seven states of the prototype
            ("em dia", on(), Sync),
            ("você hospedando", Inputs { game_running: true, lock: Holder::Me, ..on() }, Me),
            ("amigo hospedando", Inputs { lock: Holder::Other, ..on() }, Friend),
            ("baixando do amigo", Inputs { behind: true, transfer: down, ..on() }, Behind),
            ("enviando", Inputs { transfer: up, ..on() }, Up),
            ("divergiu (servidor)", Inputs { server_diverged: true, ..on() }, Diverged),
            ("sem servidor", Inputs::default(), Offline),
            // conflicts
            ("behind, pull about to start", Inputs { behind: true, ..on() }, Behind),
            ("both changed: local edits on an old base", Inputs { behind: true, local_changed: true, ..on() }, Diverged),
            ("local edits on the head are just unsent", Inputs { local_changed: true, ..on() }, Sync),
            ("autosave going up while hosting", Inputs { game_running: true, lock: Holder::Me, transfer: up, ..on() }, Me),
            ("a final upload once the game closed", Inputs { lock: Holder::Me, transfer: up, ..on() }, Up),
            ("upload beats diverged", Inputs { server_diverged: true, transfer: up, ..on() }, Up),
            ("diverged beats hosting", Inputs { server_diverged: true, game_running: true, lock: Holder::Me, ..on() }, Diverged),
            ("joined the friend's game", Inputs { game_running: true, lock: Holder::Other, ..on() }, Friend),
            ("friend hosting while behind: no pull", Inputs { behind: true, lock: Holder::Other, ..on() }, Friend),
            ("offline while playing", Inputs { game_running: true, lock: Holder::Me, ..Default::default() }, Offline),
            ("offline hides a stale lock", Inputs { lock: Holder::Other, ..Default::default() }, Offline),
            ("offline hides a stale divergence", Inputs { server_diverged: true, ..Default::default() }, Offline),
            ("offline with an upload retrying", Inputs { transfer: up, ..Default::default() }, Up),
            ("game open with the lease not yet taken", Inputs { game_running: true, ..on() }, Sync),
        ];
        for (name, inputs, want) in rows {
            assert_eq!(derive(&inputs), want, "{name}");
        }
    }
}
