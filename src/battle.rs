//! Turn-based battle system.
//!
//! Couples [`crate::ability`] (attributes / abilities / effects) with a phase
//! state machine and speed-ordered command resolution. The runner is driven
//! one turn at a time via [`TurnBattleRunner::run_turn`], which takes the
//! ally commands the caller has collected (UI / input) and asks a
//! [`BattleAi`] for enemy commands.
//!
//! ## Quick start
//!
//! ```
//! use alice_game_engine::ability::{Attribute, AttributeSet};
//! use alice_game_engine::battle::{
//!     BattleAction, BattleCommand, BattleResult, Battler, Party, RandomAi, Team,
//!     TurnBattleRunner,
//! };
//!
//! fn make_battler(name: &str, hp: f32, atk: f32, speed: f32, team: Team) -> Battler {
//!     let mut attrs = AttributeSet::new();
//!     attrs.add(Attribute::new("hp", hp, 0.0, hp));
//!     attrs.add(Attribute::new("atk", atk, 0.0, 999.0));
//!     attrs.add(Attribute::new("def", 0.0, 0.0, 999.0));
//!     attrs.add(Attribute::new("speed", speed, 0.0, 999.0));
//!     Battler::new(name, attrs, team)
//! }
//!
//! let allies = Party::new(vec![make_battler("Hero", 80.0, 12.0, 10.0, Team::Ally)]);
//! let enemies = Party::new(vec![make_battler("Slime", 25.0, 4.0, 4.0, Team::Enemy)]);
//! let mut runner = TurnBattleRunner::new(allies, enemies);
//! let mut ai = RandomAi::new(1);
//! loop {
//!     let cmds = vec![BattleCommand {
//!         actor_idx: 0,
//!         action: BattleAction::Attack { target_idx: 0 },
//!     }];
//!     match runner.run_turn(cmds, &mut ai) {
//!         BattleResult::Win => break,
//!         BattleResult::Lose | BattleResult::Fled => break,
//!         BattleResult::Ongoing => continue,
//!     }
//! }
//! ```

use crate::ability::{AbilitySystem, AttributeSet};

/// Which side of the battle a [`Battler`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    Ally,
    Enemy,
}

/// A single combatant. Wraps an [`AttributeSet`] with name + side metadata.
///
/// Conventional attribute names used by the default action handlers:
/// `hp` (current life), `atk` (attack power), `def` (defense), `speed`
/// (turn order). Providers may add `mp` or custom resource attributes too.
///
/// `cell` is optional grid coordinates for tactical RPGs; ignored by the
/// default action handlers but populated by the `Move` action and
/// inspectable by custom AIs.
/// `attack_range` is the Chebyshev range at which this battler can land
/// an `Attack`. Default `1` (melee). Bows/spells set higher values.
#[derive(Debug, Clone)]
pub struct Battler {
    pub name: String,
    pub attrs: AttributeSet,
    pub team: Team,
    pub defending: bool,
    pub cell: GridCell,
    pub attack_range: u32,
}

impl Battler {
    #[must_use]
    pub fn new(name: &str, attrs: AttributeSet, team: Team) -> Self {
        Self {
            name: name.to_string(),
            attrs,
            team,
            defending: false,
            cell: GridCell::default(),
            attack_range: 1,
        }
    }

    /// Builder: set this battler's starting grid cell.
    #[must_use]
    pub const fn with_cell(mut self, cell: GridCell) -> Self {
        self.cell = cell;
        self
    }

    /// Builder: set this battler's Chebyshev attack range.
    #[must_use]
    pub const fn with_attack_range(mut self, range: u32) -> Self {
        self.attack_range = range;
        self
    }

    /// True if the battler still has positive `hp`.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.attrs.value("hp") > 0.0
    }

    /// Speed value used for turn ordering. Defaults to 0 if attribute absent.
    #[must_use]
    pub fn speed(&self) -> f32 {
        self.attrs.value("speed")
    }
}

/// A side of the battle. Owns a vec of battlers; alive count drives end check.
#[derive(Debug, Clone, Default)]
pub struct Party {
    pub battlers: Vec<Battler>,
}

impl Party {
    #[must_use]
    pub const fn new(battlers: Vec<Battler>) -> Self {
        Self { battlers }
    }

    #[must_use]
    pub fn living_count(&self) -> usize {
        self.battlers.iter().filter(|b| b.is_alive()).count()
    }

    #[must_use]
    pub fn living_indices(&self) -> Vec<usize> {
        self.battlers
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_alive())
            .map(|(i, _)| i)
            .collect()
    }

    #[must_use]
    pub fn is_wiped(&self) -> bool {
        self.living_count() == 0
    }
}

/// Action a battler can take on their turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleAction {
    /// Basic physical attack — `atk` of actor vs `def` of target.
    Attack { target_idx: usize },
    /// Use a registered [`crate::ability::Ability`] on a target.
    UseAbility {
        ability_name: String,
        target_idx: usize,
    },
    /// Halve incoming damage this turn (sets `defending = true`).
    Defend,
    /// Attempt to flee the battle. Succeeds if total ally speed > total enemy.
    Flee,
    /// Move on the grid map (only meaningful when the runner has a
    /// [`crate::navmesh::NavMesh`] attached). `dx`/`dy` are signed cell
    /// deltas (often `-1`/`0`/`1`).
    Move { dx: i32, dy: i32 },
}

/// Grid coordinates for a battler on a tactical map. Optional — only
/// used by RPGs that want positional combat (attack range / movement).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GridCell {
    pub x: i32,
    pub y: i32,
}

impl GridCell {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Chebyshev (king-move) distance — 1 cell per N/E/S/W or diagonal.
    #[must_use]
    pub fn chebyshev(self, other: Self) -> u32 {
        (self.x - other.x)
            .unsigned_abs()
            .max((self.y - other.y).unsigned_abs())
    }

    /// Manhattan distance.
    #[must_use]
    pub const fn manhattan(self, other: Self) -> u32 {
        (self.x - other.x).unsigned_abs() + (self.y - other.y).unsigned_abs()
    }
}

/// A queued action: who is doing what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattleCommand {
    pub actor_idx: usize,
    pub action: BattleAction,
}

/// Internal phase state of the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattlePhase {
    Begin,
    AwaitingCommands,
    ExecuteTurn,
    CheckEnd,
    End,
}

/// Outcome reported by [`TurnBattleRunner::run_turn`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleResult {
    Ongoing,
    Win,
    Lose,
    Fled,
}

/// Strategy for picking enemy actions each turn.
pub trait BattleAi: Send {
    /// Choose an action for the given enemy battler.
    /// `enemy_idx` is the index within the enemy party.
    fn decide(&mut self, enemy_idx: usize, enemy_party: &Party, ally_party: &Party)
        -> BattleAction;
}

/// Linear-congruential RNG `BattleAi` — picks a random living ally to attack.
#[derive(Debug, Clone)]
pub struct RandomAi {
    state: u32,
}

impl RandomAi {
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    const fn next(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        self.state
    }
}

impl BattleAi for RandomAi {
    fn decide(&mut self, _enemy_idx: usize, _enemy: &Party, ally: &Party) -> BattleAction {
        let alive = ally.living_indices();
        if alive.is_empty() {
            return BattleAction::Defend;
        }
        let pick = (self.next() as usize) % alive.len();
        BattleAction::Attack {
            target_idx: alive[pick],
        }
    }
}

/// Turn-based battle orchestrator.
///
/// Owns two [`Party`] instances and an [`AbilitySystem`] shared by both teams.
/// Each call to [`Self::run_turn`] advances the battle by exactly one round:
///   1. Allies' commands (passed in) are queued.
///   2. Enemy commands are decided by the [`BattleAi`].
///   3. All commands are resolved in descending `speed` order.
///   4. End condition is checked; ongoing parties stay alive.
pub struct TurnBattleRunner {
    pub allies: Party,
    pub enemies: Party,
    pub ability_system: AbilitySystem,
    pub turn: u32,
    pub phase: BattlePhase,
    log: Vec<String>,
    result: BattleResult,
}

impl TurnBattleRunner {
    #[must_use]
    pub const fn new(allies: Party, enemies: Party) -> Self {
        Self {
            allies,
            enemies,
            ability_system: AbilitySystem::new(),
            turn: 0,
            phase: BattlePhase::Begin,
            log: Vec::new(),
            result: BattleResult::Ongoing,
        }
    }

    /// Adds an entry to the battle log.
    pub fn push_log(&mut self, msg: impl Into<String>) {
        self.log.push(msg.into());
    }

    /// Read-only access to the running log.
    #[must_use]
    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// Current battle result (`Ongoing` until end condition triggers).
    #[must_use]
    pub const fn result(&self) -> BattleResult {
        self.result
    }

    /// Advance the battle by one full round.
    ///
    /// `ally_commands` should contain one [`BattleCommand`] per *living* ally.
    /// The runner asks `ai` to decide an action for each living enemy.
    ///
    /// Returns the post-turn [`BattleResult`].
    pub fn run_turn(
        &mut self,
        ally_commands: Vec<BattleCommand>,
        ai: &mut dyn BattleAi,
    ) -> BattleResult {
        if self.result != BattleResult::Ongoing {
            return self.result;
        }

        self.turn += 1;
        self.phase = BattlePhase::AwaitingCommands;

        // Reset defending flags from previous turn.
        for b in &mut self.allies.battlers {
            b.defending = false;
        }
        for b in &mut self.enemies.battlers {
            b.defending = false;
        }

        // Build queue: ally commands + enemy AI decisions
        let mut queue: Vec<(Team, BattleCommand, f32)> = Vec::new();
        for cmd in ally_commands {
            if let Some(b) = self.allies.battlers.get(cmd.actor_idx) {
                if b.is_alive() {
                    let spd = b.speed();
                    queue.push((Team::Ally, cmd, spd));
                }
            }
        }
        for idx in self.enemies.living_indices() {
            let action = ai.decide(idx, &self.enemies, &self.allies);
            let spd = self.enemies.battlers[idx].speed();
            queue.push((
                Team::Enemy,
                BattleCommand {
                    actor_idx: idx,
                    action,
                },
                spd,
            ));
        }

        // Resolve Defend / Flee first (apply flag / try escape) then sort by speed.
        self.phase = BattlePhase::ExecuteTurn;

        // Pre-pass: mark defenders so attacks hitting them in same turn are halved.
        for (team, cmd, _) in &queue {
            if matches!(cmd.action, BattleAction::Defend) {
                let party = match team {
                    Team::Ally => &mut self.allies,
                    Team::Enemy => &mut self.enemies,
                };
                if let Some(b) = party.battlers.get_mut(cmd.actor_idx) {
                    b.defending = true;
                    self.log
                        .push(format!("{} braces for impact (defending).", b.name));
                }
            }
        }

        // Flee attempt: if any ally flees and ally side has more total speed.
        for (team, cmd, _) in &queue {
            if matches!(team, Team::Ally) && matches!(cmd.action, BattleAction::Flee) {
                let ally_spd: f32 = self.allies.battlers.iter().map(Battler::speed).sum();
                let enemy_spd: f32 = self.enemies.battlers.iter().map(Battler::speed).sum();
                if ally_spd > enemy_spd {
                    self.log.push("Fled successfully.".to_string());
                    self.result = BattleResult::Fled;
                    self.phase = BattlePhase::End;
                    return self.result;
                }
                self.log.push("Flee failed.".to_string());
            }
        }

        // Sort by speed descending; stable to keep insertion order for ties.
        queue.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        for (team, cmd, _) in queue {
            // Skip Defend / Flee — already handled.
            if matches!(cmd.action, BattleAction::Defend | BattleAction::Flee) {
                continue;
            }
            self.resolve_command(team, &cmd);
            if self.is_end() {
                break;
            }
        }

        self.phase = BattlePhase::CheckEnd;
        self.result = self.evaluate_end();
        if matches!(self.result, BattleResult::Ongoing) {
            self.phase = BattlePhase::AwaitingCommands;
        } else {
            self.phase = BattlePhase::End;
        }
        // Tick ability cooldowns. Effect application to a specific battler is
        // done inline in `do_use_ability`; we don't use AbilitySystem's
        // `active_effects` bucket because it lacks per-target routing
        // (multi-turn effects will be added in a later phase).
        for ability in &mut self.ability_system.abilities {
            ability.tick_cooldown();
        }
        self.result
    }

    fn resolve_command(&mut self, actor_team: Team, cmd: &BattleCommand) {
        match &cmd.action {
            BattleAction::Attack { target_idx } => {
                self.do_attack(actor_team, cmd.actor_idx, *target_idx);
            }
            BattleAction::UseAbility {
                ability_name,
                target_idx,
            } => {
                self.do_use_ability(actor_team, cmd.actor_idx, ability_name, *target_idx);
            }
            BattleAction::Defend | BattleAction::Flee => { /* handled in pre-pass */ }
            BattleAction::Move { dx, dy } => {
                self.do_move(actor_team, cmd.actor_idx, *dx, *dy);
            }
        }
    }

    fn do_move(&mut self, actor_team: Team, actor_idx: usize, dx: i32, dy: i32) {
        let party = match actor_team {
            Team::Ally => &mut self.allies,
            Team::Enemy => &mut self.enemies,
        };
        let Some(actor) = party.battlers.get_mut(actor_idx) else {
            return;
        };
        if !actor.is_alive() {
            return;
        }
        let old = actor.cell;
        actor.cell = GridCell::new(old.x + dx, old.y + dy);
        self.log.push(format!(
            "{} moves to ({}, {}).",
            actor.name, actor.cell.x, actor.cell.y
        ));
    }

    fn do_attack(&mut self, actor_team: Team, actor_idx: usize, target_idx: usize) {
        let (actor_atk, actor_name, actor_cell, actor_range) = {
            let actor_party = match actor_team {
                Team::Ally => &self.allies,
                Team::Enemy => &self.enemies,
            };
            match actor_party.battlers.get(actor_idx) {
                Some(a) if a.is_alive() => {
                    (a.attrs.value("atk"), a.name.clone(), a.cell, a.attack_range)
                }
                _ => return,
            }
        };
        let target_team = match actor_team {
            Team::Ally => Team::Enemy,
            Team::Enemy => Team::Ally,
        };
        let target_party = match target_team {
            Team::Ally => &mut self.allies,
            Team::Enemy => &mut self.enemies,
        };
        let Some(target) = target_party.battlers.get_mut(target_idx) else {
            return;
        };
        if !target.is_alive() {
            return;
        }
        // Grid range check: if both have non-default cells, enforce
        // attack_range as Chebyshev distance.
        let target_cell = target.cell;
        let default_cell = GridCell::default();
        if (actor_cell != default_cell || target_cell != default_cell)
            && actor_cell.chebyshev(target_cell) > actor_range
        {
            self.log.push(format!(
                "{actor_name} is too far from {} to attack ({} > {actor_range}).",
                target.name,
                actor_cell.chebyshev(target_cell)
            ));
            return;
        }
        let def = target.attrs.value("def");
        let mut raw = (actor_atk - def).max(1.0);
        if target.defending {
            raw *= 0.5;
        }
        target.attrs.modify("hp", -raw);
        let msg = format!(
            "{} attacks {} for {:.0} damage. ({} HP left)",
            actor_name,
            target.name,
            raw,
            target.attrs.value("hp").max(0.0)
        );
        if target.is_alive() {
            self.log.push(msg);
        } else {
            let down = format!("{} is defeated!", target.name);
            self.log.push(msg);
            self.log.push(down);
        }
    }

    fn do_use_ability(
        &mut self,
        actor_team: Team,
        actor_idx: usize,
        ability_name: &str,
        target_idx: usize,
    ) {
        // Pay cost from actor, take the resulting effect template, apply to target.
        let (actor_name, mut effect) = {
            let actor_party = match actor_team {
                Team::Ally => &mut self.allies,
                Team::Enemy => &mut self.enemies,
            };
            let Some(actor) = actor_party.battlers.get_mut(actor_idx) else {
                return;
            };
            if !actor.is_alive() {
                return;
            }
            let actor_name = actor.name.clone();
            let Some(ability) = self
                .ability_system
                .abilities
                .iter_mut()
                .find(|a| a.name == ability_name)
            else {
                self.log
                    .push(format!("{actor_name} has no ability '{ability_name}'."));
                return;
            };
            let Some(effect) = ability.activate(&mut actor.attrs) else {
                self.log
                    .push(format!("{actor_name} fails to use {ability_name}."));
                return;
            };
            (actor_name, effect)
        };

        let target_team = match actor_team {
            Team::Ally => Team::Enemy,
            Team::Enemy => Team::Ally,
        };
        let target_party = match target_team {
            Team::Ally => &mut self.allies,
            Team::Enemy => &mut self.enemies,
        };
        if let Some(target) = target_party.battlers.get_mut(target_idx) {
            if target.is_alive() {
                effect.apply(&mut target.attrs);
                self.log.push(format!(
                    "{actor_name} uses {ability_name} on {}.",
                    target.name
                ));
            }
        }
    }

    fn is_end(&self) -> bool {
        self.allies.is_wiped() || self.enemies.is_wiped()
    }

    fn evaluate_end(&self) -> BattleResult {
        if self.enemies.is_wiped() {
            BattleResult::Win
        } else if self.allies.is_wiped() {
            BattleResult::Lose
        } else {
            BattleResult::Ongoing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::{Ability, Attribute, AttributeModifier, GameplayEffect};

    fn make_battler(name: &str, hp: f32, atk: f32, def: f32, speed: f32, team: Team) -> Battler {
        let mut attrs = AttributeSet::new();
        attrs.add(Attribute::new("hp", hp, 0.0, hp));
        attrs.add(Attribute::new("mp", 30.0, 0.0, 30.0));
        attrs.add(Attribute::new("atk", atk, 0.0, 999.0));
        attrs.add(Attribute::new("def", def, 0.0, 999.0));
        attrs.add(Attribute::new("speed", speed, 0.0, 999.0));
        Battler::new(name, attrs, team)
    }

    #[test]
    fn battler_is_alive_when_hp_positive() {
        let mut b = make_battler("A", 10.0, 5.0, 0.0, 5.0, Team::Ally);
        assert!(b.is_alive());
        b.attrs.modify("hp", -10.0);
        assert!(!b.is_alive());
    }

    #[test]
    fn party_living_count_excludes_dead() {
        let mut p = Party::new(vec![
            make_battler("A", 10.0, 0.0, 0.0, 0.0, Team::Ally),
            make_battler("B", 10.0, 0.0, 0.0, 0.0, Team::Ally),
        ]);
        assert_eq!(p.living_count(), 2);
        p.battlers[0].attrs.modify("hp", -20.0);
        assert_eq!(p.living_count(), 1);
        assert!(!p.is_wiped());
        p.battlers[1].attrs.modify("hp", -20.0);
        assert!(p.is_wiped());
    }

    #[test]
    fn attack_reduces_target_hp() {
        let allies = Party::new(vec![make_battler(
            "Hero",
            30.0,
            12.0,
            0.0,
            10.0,
            Team::Ally,
        )]);
        let enemies = Party::new(vec![make_battler(
            "Slime",
            30.0,
            3.0,
            0.0,
            4.0,
            Team::Enemy,
        )]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Attack { target_idx: 0 },
            }],
            &mut ai,
        );
        assert!(r.enemies.battlers[0].attrs.value("hp") < 30.0);
    }

    #[test]
    fn defend_halves_incoming_damage() {
        let allies = Party::new(vec![make_battler("Hero", 30.0, 0.0, 0.0, 1.0, Team::Ally)]);
        let enemies = Party::new(vec![make_battler(
            "Slime",
            30.0,
            12.0,
            0.0,
            10.0,
            Team::Enemy,
        )]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(7);
        r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Defend,
            }],
            &mut ai,
        );
        let hp_loss = 30.0 - r.allies.battlers[0].attrs.value("hp");
        // base damage = 12 - 0 = 12, defending halves to 6
        assert!(
            (hp_loss - 6.0).abs() < 0.5,
            "expected ~6 dmg, got {hp_loss}"
        );
    }

    #[test]
    fn win_when_enemies_wiped() {
        let allies = Party::new(vec![make_battler(
            "Hero",
            50.0,
            100.0,
            0.0,
            10.0,
            Team::Ally,
        )]);
        let enemies = Party::new(vec![make_battler("Slime", 5.0, 1.0, 0.0, 1.0, Team::Enemy)]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        let result = r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Attack { target_idx: 0 },
            }],
            &mut ai,
        );
        assert_eq!(result, BattleResult::Win);
        assert_eq!(r.phase, BattlePhase::End);
    }

    #[test]
    fn lose_when_allies_wiped() {
        let allies = Party::new(vec![make_battler("Hero", 5.0, 1.0, 0.0, 1.0, Team::Ally)]);
        let enemies = Party::new(vec![make_battler(
            "Boss",
            50.0,
            100.0,
            0.0,
            10.0,
            Team::Enemy,
        )]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        let result = r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Attack { target_idx: 0 },
            }],
            &mut ai,
        );
        assert_eq!(result, BattleResult::Lose);
    }

    #[test]
    fn flee_succeeds_when_ally_faster() {
        let allies = Party::new(vec![make_battler(
            "Hero",
            30.0,
            1.0,
            0.0,
            100.0,
            Team::Ally,
        )]);
        let enemies = Party::new(vec![make_battler(
            "Slime",
            30.0,
            1.0,
            0.0,
            1.0,
            Team::Enemy,
        )]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        let result = r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Flee,
            }],
            &mut ai,
        );
        assert_eq!(result, BattleResult::Fled);
    }

    #[test]
    fn flee_fails_when_enemy_faster() {
        let allies = Party::new(vec![make_battler("Hero", 30.0, 1.0, 0.0, 1.0, Team::Ally)]);
        let enemies = Party::new(vec![make_battler(
            "Boss",
            30.0,
            1.0,
            0.0,
            100.0,
            Team::Enemy,
        )]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        let result = r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Flee,
            }],
            &mut ai,
        );
        assert_eq!(result, BattleResult::Ongoing);
    }

    #[test]
    fn faster_battler_acts_first() {
        // Hero (speed 100) attacks Slime (speed 1) for lethal damage; Slime
        // should be dead before it can act. Confirmed by ally HP still full.
        let allies = Party::new(vec![make_battler(
            "Hero",
            30.0,
            100.0,
            0.0,
            100.0,
            Team::Ally,
        )]);
        let enemies = Party::new(vec![make_battler(
            "Slime",
            5.0,
            100.0,
            0.0,
            1.0,
            Team::Enemy,
        )]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Attack { target_idx: 0 },
            }],
            &mut ai,
        );
        // Hero shouldn't take damage because Slime was killed first.
        assert!((r.allies.battlers[0].attrs.value("hp") - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn dead_battler_doesnt_act() {
        let mut hero = make_battler("Hero", 30.0, 12.0, 0.0, 10.0, Team::Ally);
        hero.attrs.modify("hp", -30.0);
        let allies = Party::new(vec![hero]);
        let enemies = Party::new(vec![make_battler(
            "Slime",
            30.0,
            1.0,
            0.0,
            4.0,
            Team::Enemy,
        )]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        let result = r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Attack { target_idx: 0 },
            }],
            &mut ai,
        );
        // Hero is already dead → lose immediately
        assert_eq!(result, BattleResult::Lose);
    }

    #[test]
    fn random_ai_picks_living_target() {
        let allies = Party::new(vec![make_battler("Hero", 30.0, 1.0, 0.0, 1.0, Team::Ally)]);
        let enemies = Party::new(vec![make_battler(
            "Slime",
            30.0,
            5.0,
            0.0,
            5.0,
            Team::Enemy,
        )]);
        let mut ai = RandomAi::new(42);
        let action = ai.decide(0, &enemies, &allies);
        // Only living ally is idx 0 — AI must pick 0
        assert_eq!(action, BattleAction::Attack { target_idx: 0 });
    }

    #[test]
    fn ai_defends_when_no_targets() {
        let mut hero = make_battler("Hero", 30.0, 1.0, 0.0, 1.0, Team::Ally);
        hero.attrs.modify("hp", -30.0);
        let allies = Party::new(vec![hero]);
        let enemies = Party::new(vec![make_battler(
            "Slime",
            30.0,
            5.0,
            0.0,
            5.0,
            Team::Enemy,
        )]);
        let mut ai = RandomAi::new(1);
        let action = ai.decide(0, &enemies, &allies);
        assert_eq!(action, BattleAction::Defend);
    }

    #[test]
    fn use_ability_applies_effect() {
        let mut allies = Party::new(vec![make_battler("Mage", 20.0, 1.0, 0.0, 10.0, Team::Ally)]);
        allies.battlers[0].attrs.modify("mp", 50.0); // ensure can pay
        let enemies = Party::new(vec![make_battler(
            "Slime",
            30.0,
            1.0,
            0.0,
            1.0,
            Team::Enemy,
        )]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        r.ability_system.add_ability(Ability::new(
            "fire",
            0,
            "mp",
            5.0,
            GameplayEffect::instant("burn", vec![AttributeModifier::flat("hp", -15.0)]),
        ));
        let mut ai = RandomAi::new(1);
        r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::UseAbility {
                    ability_name: "fire".to_string(),
                    target_idx: 0,
                },
            }],
            &mut ai,
        );
        // Slime took 15 damage from fire
        assert!(r.enemies.battlers[0].attrs.value("hp") < 30.0);
    }

    #[test]
    fn log_records_actions() {
        let allies = Party::new(vec![make_battler("Hero", 30.0, 5.0, 0.0, 10.0, Team::Ally)]);
        let enemies = Party::new(vec![make_battler(
            "Slime",
            30.0,
            1.0,
            0.0,
            1.0,
            Team::Enemy,
        )]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Attack { target_idx: 0 },
            }],
            &mut ai,
        );
        assert!(!r.log().is_empty());
        assert!(r.log().iter().any(|s| s.contains("Hero attacks Slime")));
    }

    #[test]
    fn result_persists_after_end() {
        let allies = Party::new(vec![make_battler(
            "Hero",
            50.0,
            100.0,
            0.0,
            10.0,
            Team::Ally,
        )]);
        let enemies = Party::new(vec![make_battler("Slime", 5.0, 1.0, 0.0, 1.0, Team::Enemy)]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Attack { target_idx: 0 },
            }],
            &mut ai,
        );
        assert_eq!(r.result(), BattleResult::Win);
        // Subsequent calls do nothing
        let r2 = r.run_turn(vec![], &mut ai);
        assert_eq!(r2, BattleResult::Win);
    }

    // -----------------------------------------------------------------------
    // Grid / range tests
    // -----------------------------------------------------------------------

    #[test]
    fn grid_cell_distances() {
        let a = GridCell::new(0, 0);
        let b = GridCell::new(3, 4);
        assert_eq!(a.chebyshev(b), 4);
        assert_eq!(a.manhattan(b), 7);
    }

    #[test]
    fn melee_blocked_by_distance() {
        let hero = make_battler("Hero", 30.0, 12.0, 0.0, 10.0, Team::Ally)
            .with_cell(GridCell::new(0, 0))
            .with_attack_range(1);
        let allies = Party::new(vec![hero]);
        let mut slime = make_battler("Slime", 30.0, 3.0, 0.0, 4.0, Team::Enemy);
        slime.cell = GridCell::new(5, 0);
        let enemies = Party::new(vec![slime]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Attack { target_idx: 0 },
            }],
            &mut ai,
        );
        // Out of range — no damage taken.
        assert!((r.enemies.battlers[0].attrs.value("hp") - 30.0).abs() < f32::EPSILON);
        assert!(r.log().iter().any(|s| s.contains("too far")));
    }

    #[test]
    fn ranged_attack_within_range() {
        let archer = make_battler("Archer", 30.0, 12.0, 0.0, 10.0, Team::Ally)
            .with_cell(GridCell::new(0, 0))
            .with_attack_range(5);
        let allies = Party::new(vec![archer]);
        let mut goblin = make_battler("Goblin", 30.0, 3.0, 0.0, 4.0, Team::Enemy);
        goblin.cell = GridCell::new(4, 2);
        let enemies = Party::new(vec![goblin]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Attack { target_idx: 0 },
            }],
            &mut ai,
        );
        // Range 5 ≥ Chebyshev 4 — should land.
        assert!(r.enemies.battlers[0].attrs.value("hp") < 30.0);
    }

    #[test]
    fn move_action_updates_cell() {
        let hero = make_battler("Hero", 30.0, 12.0, 0.0, 10.0, Team::Ally)
            .with_cell(GridCell::new(2, 2))
            .with_attack_range(1);
        let allies = Party::new(vec![hero]);
        let enemies = Party::new(vec![make_battler(
            "Slime",
            30.0,
            3.0,
            0.0,
            4.0,
            Team::Enemy,
        )]);
        let mut r = TurnBattleRunner::new(allies, enemies);
        let mut ai = RandomAi::new(1);
        r.run_turn(
            vec![BattleCommand {
                actor_idx: 0,
                action: BattleAction::Move { dx: 1, dy: -1 },
            }],
            &mut ai,
        );
        assert_eq!(r.allies.battlers[0].cell, GridCell::new(3, 1));
    }
}
