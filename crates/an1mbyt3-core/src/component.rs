//! Components: reusable animated systems with parameters and overrides —
//! AN1MBYT3_BUILD_PLAN.md §4 ("Components allow reusable animated systems
//! with parameters and overrides") and §10 Phase 7's exit criterion ("A
//! component instantiated twice with different parameter overrides renders
//! both distinctly").
//!
//! Scope for this pass: a component is a single shape's path plus a default
//! fill color. Two ways an instance can end up a different color than the
//! definition's default, matching how design tools distinguish an ad hoc
//! tweak from a reusable, named preset:
//! - A direct [`ComponentInstance::fill_color_override`] — one-off, only
//!   ever affects that one instance.
//! - A [`ComponentVariant`] referenced by name — authored once on the
//!   [`ComponentDefinition`] (e.g. "Primary"/"Secondary"), then any number of
//!   instances can swap to it by name. Still just one overridable property
//!   (fill color) — a variant with multiple properties, or properties beyond
//!   color, is the same mechanism extended, not a different one.
//!
//! This is deliberately narrower than §5's full vision ("reusable components
//! and nested animations support design-system-like motion assets" — a
//! component with its own internal node tree, multiple parameter types, and
//! *animations intrinsic to the component itself*) — proving the
//! instantiate-with-overrides/variants mechanism (one definition, many
//! instances, each visually distinct, some sharing a named preset) is this
//! pass's vertical slice. "Nested animations" specifically — each instance's
//! own node already has a real, independent `NodeId` (see
//! `Artboard::add_node`'s callers in `crates/an1mbyt3-renderer/examples/components.rs`),
//! so any external `an1mbyt3_runtime::AnimationClip`/`Playback` can already
//! target one instance's node without affecting another's — proven in that
//! same example rather than requiring a new field here. `ComponentDefinition`
//! deliberately does NOT hold an `AnimationClip` itself: `an1mbyt3-core`
//! has no dependency on `an1mbyt3-runtime` (the crate graph's arrows flow
//! one way — §3), so "a component's intrinsic animation" has to be
//! orchestrated from the runtime/example layer, the same way the top-level
//! state machine (`an1mbyt3_runtime::StateMachine`) already drives
//! `an1mbyt3-core` nodes from outside rather than living inside them.

use serde::{Deserialize, Serialize};

use crate::{ids::ComponentId, path::Path};

/// A named, reusable override preset — authored once on a
/// [`ComponentDefinition`], applied to any number of instances by name via
/// [`ComponentInstance::variant`]. The Figma-style distinction from a plain
/// [`ComponentInstance::fill_color_override`]: a variant is a shared,
/// swappable preset (rename it, and every instance using it updates); a
/// direct override is a one-off tweak to a single instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentVariant {
    pub name: String,
    pub fill_color: [f32; 4],
}

/// A reusable component definition: authored once, instantiated many times
/// via [`crate::NodeKind::Instance`]. Lives on [`crate::Document`] (not on
/// an [`crate::Artboard`]) because a component is meant to be usable from
/// any artboard in the document, unlike a plain [`crate::Node`], which only
/// exists within the one artboard that created it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentDefinition {
    pub name: String,
    pub path: Path,
    pub default_fill_color: [f32; 4],
    /// Named override presets authors can define once and reference from
    /// many instances (`ComponentInstance::variant`) — `Vec`, not a `HashMap`
    /// keyed by name, so definition order is preserved for UI listing (a
    /// future variant-picker dropdown), matching this workspace's existing
    /// "Vec preserves author order, lookups are a linear scan over a small
    /// list" convention (e.g. `Artboard::children_of`).
    #[serde(default)]
    pub variants: Vec<ComponentVariant>,
    /// Soft-delete flag — same reasoning as `Animation::deleted`/`Node::deleted`/
    /// `StateMachineDef::deleted`: undo/redo needs a deleted component's
    /// `ComponentId` to stay valid for any earlier command still on the undo
    /// stack (e.g. an instance created from it before the delete). `#[serde(default)]`
    /// for the same back-compat reason every additive field here gets one.
    #[serde(default)]
    pub deleted: bool,
}

impl ComponentDefinition {
    /// Looks up a variant by name — `None` if this definition has none by
    /// that name (a caller-supplied name that doesn't exist, e.g. a stale
    /// reference after a variant was renamed/removed).
    pub fn variant(&self, name: &str) -> Option<&ComponentVariant> {
        self.variants.iter().find(|v| v.name == name)
    }
}

/// One instance of a [`ComponentDefinition`] — the payload of
/// [`crate::NodeKind::Instance`]. Resolution order for the effective fill
/// color (see [`ComponentInstance::resolved_fill_color`]): a direct
/// `fill_color_override` wins if set; otherwise a named `variant` if it
/// resolves to a real variant on the definition; otherwise the definition's
/// own `default_fill_color`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentInstance {
    pub component: ComponentId,
    pub fill_color_override: Option<[f32; 4]>,
    /// A [`ComponentVariant`] name to use instead of the definition's
    /// default — `None` means "no variant selected," not "the first
    /// variant." Ignored (falls through to the definition's default) if it
    /// doesn't match any of the definition's `variants`, rather than a
    /// resolution error — the same "degrade gracefully rather than panic on
    /// a stale reference" reasoning `trim_path`'s doc comment already
    /// applies to out-of-range trim values.
    #[serde(default)]
    pub variant: Option<String>,
}

impl ComponentInstance {
    pub fn new(component: ComponentId) -> Self {
        Self { component, fill_color_override: None, variant: None }
    }

    /// Resolves this instance's effective fill color — see this struct's
    /// doc comment for the precedence order. Takes the definition directly
    /// (rather than a `&Document` to look it up internally) so this stays a
    /// pure function independent of how/where the caller looked the
    /// definition up — matching `Command::apply`'s convention of operating
    /// on the smallest piece of state it actually needs.
    pub fn resolved_fill_color(&self, definition: &ComponentDefinition) -> [f32; 4] {
        if let Some(override_color) = self.fill_color_override {
            return override_color;
        }
        if let Some(variant_name) = &self.variant {
            if let Some(variant) = definition.variant(variant_name) {
                return variant.fill_color;
            }
        }
        definition.default_fill_color
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    fn dot_definition() -> ComponentDefinition {
        ComponentDefinition {
            name: "Dot".into(),
            path: Path::rect(20.0, 20.0),
            default_fill_color: [1.0, 0.0, 0.0, 1.0],
            variants: vec![ComponentVariant { name: "Gold".into(), fill_color: [1.0, 0.8, 0.0, 1.0] }],
            deleted: false,
        }
    }

    #[test]
    fn instance_without_override_uses_the_definitions_default() {
        let mut document = Document::new();
        let component_id = document.add_component(dot_definition());
        let definition = document.component(component_id).unwrap();

        let instance = ComponentInstance::new(component_id);
        assert_eq!(instance.resolved_fill_color(definition), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn two_instances_with_different_overrides_resolve_distinctly() {
        let mut document = Document::new();
        let component_id = document.add_component(dot_definition());
        let definition = document.component(component_id).unwrap();

        let red_instance = ComponentInstance::new(component_id);
        let mut blue_instance = ComponentInstance::new(component_id);
        blue_instance.fill_color_override = Some([0.0, 0.0, 1.0, 1.0]);

        assert_ne!(
            red_instance.resolved_fill_color(definition),
            blue_instance.resolved_fill_color(definition)
        );
    }

    #[test]
    fn instance_referencing_a_variant_resolves_to_that_variants_color() {
        let mut document = Document::new();
        let component_id = document.add_component(dot_definition());
        let definition = document.component(component_id).unwrap();

        let mut instance = ComponentInstance::new(component_id);
        instance.variant = Some("Gold".into());
        assert_eq!(instance.resolved_fill_color(definition), [1.0, 0.8, 0.0, 1.0]);
    }

    #[test]
    fn a_direct_override_wins_over_a_variant() {
        let mut document = Document::new();
        let component_id = document.add_component(dot_definition());
        let definition = document.component(component_id).unwrap();

        let mut instance = ComponentInstance::new(component_id);
        instance.variant = Some("Gold".into());
        instance.fill_color_override = Some([0.0, 1.0, 0.0, 1.0]);
        assert_eq!(instance.resolved_fill_color(definition), [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn a_variant_name_with_no_match_falls_back_to_the_default_rather_than_panicking() {
        let mut document = Document::new();
        let component_id = document.add_component(dot_definition());
        let definition = document.component(component_id).unwrap();

        let mut instance = ComponentInstance::new(component_id);
        instance.variant = Some("Nonexistent".into());
        assert_eq!(instance.resolved_fill_color(definition), definition.default_fill_color);
    }

    #[test]
    fn multiple_instances_can_share_the_same_variant() {
        let mut document = Document::new();
        let component_id = document.add_component(dot_definition());
        let definition = document.component(component_id).unwrap();

        let mut first = ComponentInstance::new(component_id);
        first.variant = Some("Gold".into());
        let mut second = ComponentInstance::new(component_id);
        second.variant = Some("Gold".into());

        assert_eq!(first.resolved_fill_color(definition), second.resolved_fill_color(definition));
    }
}
