//! User-authored "script" components — the Unity `MonoBehaviour` analog:
//! a named, reusable *type* with typed, serializable fields (authored once
//! as a [`ScriptDefinition`], the same "define once, instantiate many
//! times" shape [`crate::ComponentDefinition`] already establishes for
//! shape prefabs), attached to any node via [`crate::NodeBehavior::Script`]
//! as a [`ScriptInstance`] carrying that node's own field *values*.
//!
//! Scope for this pass: the data model and field editing an author sees in
//! the Inspector (type a field, give it a default, mark it animatable) —
//! matching the user's own ask, "serializable fields... just like Unity."
//! Two things are deliberately NOT built yet, flagged rather than silently
//! skipped:
//! - A field marked `animatable` doesn't yet have a real keyframe track to
//!   animate it with — `crate::timeline::PropertyTrack` is still a fixed
//!   enum of concrete `Node`/`NodeKind` fields (`Translation`/`FillColor`/
//!   etc.), not yet generic over an arbitrary `(behavior index, field
//!   name)` pair. The flag exists now (and is serialized, so it round-trips)
//!   so the Inspector can already show which fields *will* be keyframeable
//!   once that track type exists, without a later migration.
//! - A script's fields are pure data — nothing here executes behavior logic
//!   per frame (Unity's `Update()`). Wiring a script to the existing
//!   Blockly-style program system (`crate::program::ProgramDef`, currently
//!   state-machine-scoped only) is a separate, later vertical slice.

use serde::{Deserialize, Serialize};

use crate::ids::ScriptId;

/// A script field's value type — deliberately small and scalar-only for this
/// pass (matching `ComponentVariant`'s own "one overridable property,
/// extended later" scope note): every variant here is a type an Inspector
/// number/checkbox/color/text input can already edit with no new widget.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ScriptFieldKind {
    Float,
    Int,
    Bool,
    Color,
    Vec2,
    String,
}

/// One field's actual value — tagged so a single `Vec<(String,
/// ScriptFieldValue)>` (see [`ScriptInstance::values`]) can hold a script's
/// whole mixed-type field set without one `Vec` per type. `String` is boxed
/// nowhere special; kept last to mirror `ScriptFieldKind`'s own order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScriptFieldValue {
    Float(f32),
    Int(i32),
    Bool(bool),
    Color([f32; 4]),
    Vec2([f32; 2]),
    String(String),
}

impl ScriptFieldValue {
    /// The `ScriptFieldKind` this value is already an instance of — lets the
    /// Inspector render the right widget for a value it already has (e.g.
    /// an instance's current override) without separately consulting the
    /// field's own definition.
    pub fn kind(&self) -> ScriptFieldKind {
        match self {
            ScriptFieldValue::Float(_) => ScriptFieldKind::Float,
            ScriptFieldValue::Int(_) => ScriptFieldKind::Int,
            ScriptFieldValue::Bool(_) => ScriptFieldKind::Bool,
            ScriptFieldValue::Color(_) => ScriptFieldKind::Color,
            ScriptFieldValue::Vec2(_) => ScriptFieldKind::Vec2,
            ScriptFieldValue::String(_) => ScriptFieldKind::String,
        }
    }

    /// The zero-ish value for a freshly-added field of `kind` — what a new
    /// field starts out as before an author picks a real default, and what
    /// `ScriptInstance::sync_to_definition` fills in for a field an instance
    /// doesn't have a value for yet.
    pub fn default_for(kind: ScriptFieldKind) -> Self {
        match kind {
            ScriptFieldKind::Float => ScriptFieldValue::Float(0.0),
            ScriptFieldKind::Int => ScriptFieldValue::Int(0),
            ScriptFieldKind::Bool => ScriptFieldValue::Bool(false),
            ScriptFieldKind::Color => ScriptFieldValue::Color([1.0, 1.0, 1.0, 1.0]),
            ScriptFieldKind::Vec2 => ScriptFieldValue::Vec2([0.0, 0.0]),
            ScriptFieldKind::String => ScriptFieldValue::String(String::new()),
        }
    }
}

/// One field of a [`ScriptDefinition`] — a typed, named, (optionally)
/// animatable slot every instance of that script carries its own value for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptFieldDef {
    pub name: String,
    pub kind: ScriptFieldKind,
    pub default: ScriptFieldValue,
    /// Marks this field as one the timeline *will* be able to keyframe —
    /// see this module's own doc comment for why that capability doesn't
    /// exist yet. Stored now so the flag round-trips through `.tea`/`.teab`
    /// rather than needing a schema migration once it's wired up.
    #[serde(default)]
    pub animatable: bool,
}

/// A reusable script "type": authored once (name + ordered field list),
/// attached to any number of nodes via [`crate::NodeBehavior::Script`]. Lives
/// on [`crate::Document`] (not an [`crate::Artboard`]) — same "usable from
/// any artboard" reasoning as [`crate::ComponentDefinition`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptDefinition {
    pub name: String,
    pub fields: Vec<ScriptFieldDef>,
    /// Soft-delete flag — same reasoning as `ComponentDefinition::deleted`:
    /// an instance created from this definition before the delete needs the
    /// `ScriptId` to stay valid for any earlier command still on the undo
    /// stack.
    #[serde(default)]
    pub deleted: bool,
}

impl ScriptDefinition {
    pub fn field(&self, name: &str) -> Option<&ScriptFieldDef> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// One instance of a [`ScriptDefinition`] — the payload of
/// [`crate::NodeBehavior::Script`]. Carries this node's own value for every
/// field the definition currently has; `values` can genuinely lag behind
/// `fields` (a field added to the definition *after* this instance was
/// created) — see [`ScriptInstance::value_or_default`], which is what every
/// reader should call instead of indexing `values` directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptInstance {
    pub definition: ScriptId,
    #[serde(default)]
    pub values: Vec<(String, ScriptFieldValue)>,
}

impl ScriptInstance {
    /// A fresh instance with no overrides recorded yet — every field reads
    /// as its definition's own default until explicitly set.
    pub fn new(definition: ScriptId) -> Self {
        Self { definition, values: Vec::new() }
    }

    /// This instance's current value for `field`, or `None` if it's never
    /// been set on this particular instance (the caller falls back to the
    /// definition's own `ScriptFieldDef::default` — see
    /// `value_or_default`, the convenience that does both steps at once).
    pub fn value(&self, field: &str) -> Option<&ScriptFieldValue> {
        self.values.iter().find(|(name, _)| name == field).map(|(_, value)| value)
    }

    /// `value(field)`, falling back to `definition`'s own authored default —
    /// the one method every Inspector field editor / future runtime reader
    /// should actually call, so a field added to the definition after this
    /// instance existed still resolves instead of reading as missing.
    pub fn value_or_default<'a>(&'a self, definition: &'a ScriptDefinition, field: &str) -> Option<&'a ScriptFieldValue> {
        self.value(field).or_else(|| definition.field(field).map(|f| &f.default))
    }

    /// Records `value` for `field` on this instance, replacing whatever was
    /// there before — the Inspector's per-field edit commit. Inserts a new
    /// `(field, value)` pair if this instance had no override for it yet.
    pub fn set_value(&mut self, field: &str, value: ScriptFieldValue) {
        match self.values.iter_mut().find(|(name, _)| name == field) {
            Some((_, slot)) => *slot = value,
            None => self.values.push((field.to_string(), value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    fn counter_definition() -> ScriptDefinition {
        ScriptDefinition {
            name: "Counter".into(),
            fields: vec![
                ScriptFieldDef { name: "count".into(), kind: ScriptFieldKind::Int, default: ScriptFieldValue::Int(0), animatable: false },
                ScriptFieldDef { name: "speed".into(), kind: ScriptFieldKind::Float, default: ScriptFieldValue::Float(1.0), animatable: true },
            ],
            deleted: false,
        }
    }

    #[test]
    fn instance_with_no_overrides_resolves_every_field_to_the_definitions_default() {
        let mut document = Document::new();
        let script_id = document.add_script(counter_definition());
        let definition = document.script(script_id).unwrap();

        let instance = ScriptInstance::new(script_id);
        assert_eq!(instance.value_or_default(definition, "count"), Some(&ScriptFieldValue::Int(0)));
        assert_eq!(instance.value_or_default(definition, "speed"), Some(&ScriptFieldValue::Float(1.0)));
    }

    #[test]
    fn setting_a_value_overrides_just_that_field() {
        let mut document = Document::new();
        let script_id = document.add_script(counter_definition());
        let definition = document.script(script_id).unwrap();

        let mut instance = ScriptInstance::new(script_id);
        instance.set_value("count", ScriptFieldValue::Int(7));

        assert_eq!(instance.value_or_default(definition, "count"), Some(&ScriptFieldValue::Int(7)));
        assert_eq!(instance.value_or_default(definition, "speed"), Some(&ScriptFieldValue::Float(1.0)));
    }

    #[test]
    fn a_field_added_after_the_instance_existed_still_resolves_via_the_definitions_default() {
        let mut document = Document::new();
        let script_id = document.add_script(counter_definition());

        let instance = ScriptInstance::new(script_id);

        let definition = document.script_mut(script_id).unwrap();
        definition.fields.push(ScriptFieldDef {
            name: "label".into(),
            kind: ScriptFieldKind::String,
            default: ScriptFieldValue::String("hello".into()),
            animatable: false,
        });
        let definition = document.script(script_id).unwrap();

        assert_eq!(instance.value_or_default(definition, "label"), Some(&ScriptFieldValue::String("hello".into())));
    }

    #[test]
    fn two_instances_of_the_same_script_hold_independent_values() {
        let mut document = Document::new();
        let script_id = document.add_script(counter_definition());
        let definition = document.script(script_id).unwrap();

        let mut first = ScriptInstance::new(script_id);
        first.set_value("count", ScriptFieldValue::Int(1));
        let mut second = ScriptInstance::new(script_id);
        second.set_value("count", ScriptFieldValue::Int(2));

        assert_ne!(first.value_or_default(definition, "count"), second.value_or_default(definition, "count"));
    }
}
