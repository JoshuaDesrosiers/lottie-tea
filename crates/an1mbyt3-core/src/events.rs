//! Unity-style **events** and Rive-style **properties** — wiring a scene
//! together without writing a script.
//!
//! - [`UiEvents`] — a node's *On Click* list (Unity's `Button.onClick`): when
//!   the node's UI Button is clicked, each [`EventAction`] runs in order —
//!   show / hide / toggle a node, set one of its fields or one of its
//!   components' fields, call a component method (emit particles, play a
//!   sound, shake a tree…), fire a state machine trigger or set an input,
//!   load a scene, play a sound, set a property, or call a script function
//!   (`ctx.on('name', fn)` in a script).
//! - [`PropertyDef`] — a scene's typed, named **properties** (Rive's view
//!   model): number, bool, text, colour. One reliable source of truth that
//!   the Inspector, scripts (`ctx.props.score`), button actions and HTML UI
//!   (`data-bind` / `data-prop`) all read and write.
//! - [`PropertyBindings`] — a node component that drives the node from
//!   properties every frame: its text ("Score: {}"), position, rotation,
//!   scale, visibility, colour, or any component field.
//!
//! All of this is authored data; it runs in the game's script host (the
//! editor's Playground and exported games).
//!
//! Values are stored as strings (JSON for structured ones, e.g. a colour
//! `[1,0,0,1]`) so the binary `.teab` format stays simple and the same data
//! can feed any field.

use serde::{Deserialize, Serialize};

/// What an [`EventAction`] does. Externally tagged (`.teab`-safe).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActionKind {
    /// Shows (`true`) or hides the target.
    SetVisible { visible: bool },
    ToggleVisible,
    /// A node field: `x`, `y`, `rotation`, `scaleX`, `scaleY`, `text`,
    /// `fillColor` (`value`: a number, a string, or `[r,g,b,a]` JSON).
    SetNodeField { field: String, value: String },
    /// One field of one of the target's components (`component` is the
    /// component kind, e.g. `"Light2D"`; `field` its camelCase name, e.g.
    /// `"intensity"`; `value` JSON).
    SetComponentField { component: String, field: String, value: String },
    /// A component method: `ParticleSystem.emit`, `AudioSource.play` /
    /// `stop`, `Tree2D.shake` / `regrow`, `Cloth2D.reset`… (`arg` optional,
    /// e.g. a particle count).
    CallMethod { component: String, method: String, arg: String },
    /// Fires a state machine trigger.
    FireTrigger { input: String },
    /// Sets a state machine input (`value` as text: `true`, `3`, `"Run"`).
    SetInput { input: String, value: String },
    /// Switches scene.
    LoadScene { scene: String },
    /// Plays an imported sound (by name).
    PlaySound { sound: String },
    /// Sets a scene property (`value` as JSON / text; for numbers a leading
    /// `+` / `-` adds to the current value: `"+1"`).
    SetProperty { property: String, value: String },
    /// Calls a script function registered with `ctx.on(name, fn)`.
    CallFunction { function: String, arg: String },
}

/// One step of an event: an action on a target node (`None` = the node
/// carrying the event).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventAction {
    pub target: Option<crate::NodeId>,
    pub action: ActionKind,
}

/// A node's events — see the module docs. (On Click runs when the node's
/// UI Button is clicked.)
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiEvents {
    pub on_click: Vec<EventAction>,
}

/// A property's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PropKind {
    #[default]
    Number,
    Bool,
    Text,
    /// `[r, g, b, a]`, straight linear 0..1.
    Color,
}

/// A scene's named, typed property (Rive's view model) — see the module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyDef {
    /// The scene it belongs to.
    pub artboard: crate::ArtboardId,
    pub name: String,
    pub kind: PropKind,
    /// Its starting value as JSON (`0`, `true`, `"Ada"`, `[1,0,0,1]`).
    pub default: String,
    #[serde(default)]
    pub deleted: bool,
}

/// What a [`Binding`] drives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BindTarget {
    /// A text node's content (`format` with `{}` for the value).
    Text,
    X,
    Y,
    Rotation,
    ScaleX,
    ScaleY,
    /// Uniform scale (both axes).
    Scale,
    /// Shown when the value is truthy.
    Visible,
    /// The node's fill colour (a Color property).
    FillColor,
    /// Any component field: `component` kind + camelCase `field`.
    ComponentField { component: String, field: String },
}

/// One property → node link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub property: String,
    pub target: BindTarget,
    /// For text: how to show it, `{}` = the value (`"Score: {}"`). Empty =
    /// just the value.
    #[serde(default)]
    pub format: String,
    /// For numbers: `value * scale + offset` before applying (e.g. health
    /// 0..100 → bar scale 0..1 with scale 0.01).
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default)]
    pub offset: f32,
}

fn one() -> f32 {
    1.0
}

/// A node's property bindings — see the module docs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PropertyBindings {
    pub bindings: Vec<Binding>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_and_bindings_round_trip_through_json() {
        let events = UiEvents {
            on_click: vec![
                EventAction { target: None, action: ActionKind::SetProperty { property: "score".into(), value: "+1".into() } },
                EventAction { target: None, action: ActionKind::CallMethod { component: "ParticleSystem".into(), method: "emit".into(), arg: "20".into() } },
            ],
        };
        let json = serde_json::to_string(&events).unwrap();
        assert!(json.contains("\"onClick\""));
        assert_eq!(serde_json::from_str::<UiEvents>(&json).unwrap(), events);
        let bind = PropertyBindings { bindings: vec![Binding { property: "hp".into(), target: BindTarget::ScaleX, format: String::new(), scale: 0.01, offset: 0.0 }] };
        assert_eq!(serde_json::from_str::<PropertyBindings>(&serde_json::to_string(&bind).unwrap()).unwrap(), bind);
    }
}
