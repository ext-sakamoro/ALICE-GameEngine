//! Gameplay Ability System (UE5 GAS inspired).
//!
//! Abilities, effects, attribute modifiers, and cooldowns.
//!
//! ```rust
//! use alice_game_engine::ability::*;
//!
//! let mut attrs = AttributeSet::new();
//! attrs.add(Attribute::new("hp", 100.0, 0.0, 100.0));
//! attrs.modify("hp", -30.0);
//! assert!((attrs.value("hp") - 70.0).abs() < 0.1);
//! ```
//! Designed after UE5's GAS but with Rust enum dispatch.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Attribute
// ---------------------------------------------------------------------------

/// A named gameplay attribute (health, mana, speed, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    pub base_value: f32,
    pub current_value: f32,
    pub min_value: f32,
    pub max_value: f32,
}

impl Attribute {
    #[must_use]
    pub fn new(name: &str, base: f32, min: f32, max: f32) -> Self {
        Self {
            name: name.to_string(),
            base_value: base,
            current_value: base,
            min_value: min,
            max_value: max,
        }
    }

    /// Applies a flat modifier and clamps.
    pub fn modify(&mut self, delta: f32) {
        self.current_value = (self.current_value + delta).clamp(self.min_value, self.max_value);
    }

    /// Resets to base value.
    pub const fn reset(&mut self) {
        self.current_value = self.base_value;
    }

    /// Returns ratio (current / max), 0.0 to 1.0.
    #[must_use]
    pub fn ratio(&self) -> f32 {
        if self.max_value <= self.min_value {
            return 0.0;
        }
        ((self.current_value - self.min_value) / (self.max_value - self.min_value)).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// AttributeSet
// ---------------------------------------------------------------------------

/// A collection of named attributes for an entity.
pub struct AttributeSet {
    attrs: HashMap<String, Attribute>,
}

impl AttributeSet {
    #[must_use]
    pub fn new() -> Self {
        Self {
            attrs: HashMap::new(),
        }
    }

    pub fn add(&mut self, attr: Attribute) {
        self.attrs.insert(attr.name.clone(), attr);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Attribute> {
        self.attrs.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Attribute> {
        self.attrs.get_mut(name)
    }

    pub fn modify(&mut self, name: &str, delta: f32) -> bool {
        self.attrs.get_mut(name).is_some_and(|attr| {
            attr.modify(delta);
            true
        })
    }

    #[must_use]
    pub fn value(&self, name: &str) -> f32 {
        self.attrs.get(name).map_or(0.0, |a| a.current_value)
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.attrs.len()
    }
}

impl Default for AttributeSet {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// GameplayEffect
// ---------------------------------------------------------------------------

/// How an effect modifies attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectDuration {
    Instant,
    Duration(u32), // ticks
    Infinite,
}

/// A modifier applied by an effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeModifier {
    pub attribute: String,
    pub flat_delta: f32,
    pub multiplier: f32,
}

impl AttributeModifier {
    #[must_use]
    pub fn flat(attribute: &str, delta: f32) -> Self {
        Self {
            attribute: attribute.to_string(),
            flat_delta: delta,
            multiplier: 1.0,
        }
    }

    #[must_use]
    pub fn multiply(attribute: &str, factor: f32) -> Self {
        Self {
            attribute: attribute.to_string(),
            flat_delta: 0.0,
            multiplier: factor,
        }
    }

    /// Computes the effective delta for a given base value.
    #[must_use]
    pub fn compute(&self, base: f32) -> f32 {
        base.mul_add(self.multiplier - 1.0, self.flat_delta)
    }
}

/// A gameplay effect that modifies attributes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameplayEffect {
    pub name: String,
    pub duration: EffectDuration,
    pub modifiers: Vec<AttributeModifier>,
    pub remaining_ticks: u32,
    pub active: bool,
}

impl GameplayEffect {
    #[must_use]
    pub fn instant(name: &str, modifiers: Vec<AttributeModifier>) -> Self {
        Self {
            name: name.to_string(),
            duration: EffectDuration::Instant,
            modifiers,
            remaining_ticks: 0,
            active: true,
        }
    }

    #[must_use]
    pub fn timed(name: &str, ticks: u32, modifiers: Vec<AttributeModifier>) -> Self {
        Self {
            name: name.to_string(),
            duration: EffectDuration::Duration(ticks),
            modifiers,
            remaining_ticks: ticks,
            active: true,
        }
    }

    /// Applies this effect to an attribute set. Returns true if still active.
    pub fn apply(&mut self, attrs: &mut AttributeSet) -> bool {
        if !self.active {
            return false;
        }
        for m in &self.modifiers {
            let base = attrs.value(&m.attribute);
            let delta = m.compute(base);
            attrs.modify(&m.attribute, delta);
        }
        match self.duration {
            EffectDuration::Instant => {
                self.active = false;
                false
            }
            EffectDuration::Duration(_) => {
                self.remaining_ticks = self.remaining_ticks.saturating_sub(1);
                if self.remaining_ticks == 0 {
                    self.active = false;
                }
                self.active
            }
            EffectDuration::Infinite => true,
        }
    }
}

// ---------------------------------------------------------------------------
// Ability
// ---------------------------------------------------------------------------

/// A gameplay ability with cooldown and cost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ability {
    pub name: String,
    pub cooldown_max: u32,
    pub cooldown_current: u32,
    pub cost_attribute: String,
    pub cost_amount: f32,
    pub effect: GameplayEffect,
}

impl Ability {
    #[must_use]
    pub fn new(
        name: &str,
        cooldown: u32,
        cost_attr: &str,
        cost: f32,
        effect: GameplayEffect,
    ) -> Self {
        Self {
            name: name.to_string(),
            cooldown_max: cooldown,
            cooldown_current: 0,
            cost_attribute: cost_attr.to_string(),
            cost_amount: cost,
            effect,
        }
    }

    /// Returns true if the ability can be activated.
    #[must_use]
    pub fn can_activate(&self, attrs: &AttributeSet) -> bool {
        self.cooldown_current == 0 && attrs.value(&self.cost_attribute) >= self.cost_amount
    }

    /// Activates the ability: pays cost, starts cooldown, returns the effect.
    pub fn activate(&mut self, attrs: &mut AttributeSet) -> Option<GameplayEffect> {
        if !self.can_activate(attrs) {
            return None;
        }
        attrs.modify(&self.cost_attribute, -self.cost_amount);
        self.cooldown_current = self.cooldown_max;
        let mut effect = self.effect.clone();
        effect.active = true;
        Some(effect)
    }

    /// Ticks cooldown by 1.
    pub const fn tick_cooldown(&mut self) {
        self.cooldown_current = self.cooldown_current.saturating_sub(1);
    }

    /// Returns true if on cooldown.
    #[must_use]
    pub const fn is_on_cooldown(&self) -> bool {
        self.cooldown_current > 0
    }
}

// ---------------------------------------------------------------------------
// AbilitySystem — manages abilities + active effects
// ---------------------------------------------------------------------------

/// Manages abilities and active gameplay effects.
pub struct AbilitySystem {
    pub abilities: Vec<Ability>,
    pub active_effects: Vec<GameplayEffect>,
}

impl AbilitySystem {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            abilities: Vec::new(),
            active_effects: Vec::new(),
        }
    }

    pub fn add_ability(&mut self, ability: Ability) {
        self.abilities.push(ability);
    }

    /// Tries to activate an ability by name.
    pub fn activate(&mut self, name: &str, attrs: &mut AttributeSet) -> bool {
        for ability in &mut self.abilities {
            if ability.name == name {
                if let Some(effect) = ability.activate(attrs) {
                    self.active_effects.push(effect);
                    return true;
                }
                return false;
            }
        }
        false
    }

    /// Ticks all cooldowns and applies active effects.
    pub fn tick(&mut self, attrs: &mut AttributeSet) {
        for ability in &mut self.abilities {
            ability.tick_cooldown();
        }
        for effect in &mut self.active_effects {
            effect.apply(attrs);
        }
        self.active_effects.retain(|e| e.active);
    }

    #[must_use]
    pub const fn active_effect_count(&self) -> usize {
        self.active_effects.len()
    }
}

impl Default for AbilitySystem {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn health_set() -> AttributeSet {
        let mut set = AttributeSet::new();
        set.add(Attribute::new("health", 100.0, 0.0, 100.0));
        set.add(Attribute::new("mana", 50.0, 0.0, 100.0));
        set
    }

    #[test]
    fn attribute_modify() {
        let mut a = Attribute::new("hp", 100.0, 0.0, 100.0);
        a.modify(-30.0);
        assert!((a.current_value - 70.0).abs() < 1e-6);
    }

    #[test]
    fn attribute_clamp() {
        let mut a = Attribute::new("hp", 100.0, 0.0, 100.0);
        a.modify(50.0);
        assert_eq!(a.current_value, 100.0);
        a.modify(-200.0);
        assert_eq!(a.current_value, 0.0);
    }

    #[test]
    fn attribute_ratio() {
        let a = Attribute::new("hp", 50.0, 0.0, 100.0);
        assert!((a.ratio() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn attribute_reset() {
        let mut a = Attribute::new("hp", 100.0, 0.0, 100.0);
        a.modify(-50.0);
        a.reset();
        assert_eq!(a.current_value, 100.0);
    }

    #[test]
    fn attribute_set_basics() {
        let mut set = health_set();
        assert_eq!(set.count(), 2);
        assert!((set.value("health") - 100.0).abs() < 1e-6);
        set.modify("health", -25.0);
        assert!((set.value("health") - 75.0).abs() < 1e-6);
    }

    #[test]
    fn attribute_set_missing() {
        let set = health_set();
        assert_eq!(set.value("nonexistent"), 0.0);
    }

    #[test]
    fn modifier_flat() {
        let m = AttributeModifier::flat("hp", -10.0);
        assert_eq!(m.compute(100.0), -10.0);
    }

    #[test]
    fn modifier_multiply() {
        let m = AttributeModifier::multiply("hp", 1.5);
        assert!((m.compute(100.0) - 50.0).abs() < 1e-4);
    }

    #[test]
    fn instant_effect() {
        let mut set = health_set();
        let mut effect =
            GameplayEffect::instant("damage", vec![AttributeModifier::flat("health", -20.0)]);
        let active = effect.apply(&mut set);
        assert!(!active);
        assert!((set.value("health") - 80.0).abs() < 1e-6);
    }

    #[test]
    fn timed_effect() {
        let mut set = health_set();
        let mut effect =
            GameplayEffect::timed("regen", 3, vec![AttributeModifier::flat("health", 5.0)]);
        set.modify("health", -50.0);
        for _ in 0..3 {
            effect.apply(&mut set);
        }
        assert!(!effect.active);
        assert!((set.value("health") - 65.0).abs() < 1e-4);
    }

    #[test]
    fn ability_activate() {
        let mut set = health_set();
        let effect = GameplayEffect::instant("heal", vec![AttributeModifier::flat("health", 20.0)]);
        let mut ability = Ability::new("heal", 5, "mana", 10.0, effect);
        set.modify("health", -30.0);
        assert!(ability.can_activate(&set));
        let result = ability.activate(&mut set);
        assert!(result.is_some());
        assert!(ability.is_on_cooldown());
        assert!((set.value("mana") - 40.0).abs() < 1e-6);
    }

    #[test]
    fn ability_cooldown() {
        let mut set = health_set();
        let effect = GameplayEffect::instant("hit", vec![]);
        let mut ability = Ability::new("hit", 3, "mana", 0.0, effect);
        ability.activate(&mut set);
        assert!(!ability.can_activate(&set));
        ability.tick_cooldown();
        ability.tick_cooldown();
        ability.tick_cooldown();
        assert!(ability.can_activate(&set));
    }

    #[test]
    fn ability_insufficient_cost() {
        let mut set = health_set();
        let effect = GameplayEffect::instant("big", vec![]);
        let mut ability = Ability::new("big", 0, "mana", 999.0, effect);
        assert!(!ability.can_activate(&set));
        assert!(ability.activate(&mut set).is_none());
    }

    #[test]
    fn ability_system_tick() {
        let mut set = health_set();
        set.modify("health", -50.0);
        let mut sys = AbilitySystem::new();
        sys.add_ability(Ability::new(
            "regen",
            0,
            "mana",
            5.0,
            GameplayEffect::timed("regen", 2, vec![AttributeModifier::flat("health", 10.0)]),
        ));
        sys.activate("regen", &mut set);
        assert_eq!(sys.active_effect_count(), 1);
        sys.tick(&mut set);
        sys.tick(&mut set);
        assert_eq!(sys.active_effect_count(), 0);
        assert!(set.value("health") > 50.0);
    }

    #[test]
    fn ability_system_activate_unknown() {
        let mut set = health_set();
        let mut sys = AbilitySystem::new();
        assert!(!sys.activate("nonexistent", &mut set));
    }

    #[test]
    fn effect_duration_variants() {
        assert_eq!(EffectDuration::Instant, EffectDuration::Instant);
        assert_ne!(EffectDuration::Instant, EffectDuration::Infinite);
    }
}
