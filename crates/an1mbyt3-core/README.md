# an1mbyt3-core

The document model and scene graph — the single source of truth for the whole
project (editor, runtime, renderer, importer, exporter all read/write this model).

**Depends on:** nothing else in the workspace (bottom of the dependency graph).
**Depended on by:** every other crate, directly or transitively.

See [AN1MBYT3_BUILD_PLAN.md](../../AN1MBYT3_BUILD_PLAN.md) §3 (crate layout) and §4
(document model, node ID scheme, undo/redo architecture) for the full design.

## Status

Not yet implemented — Phase 1 (AN1MBYT3_BUILD_PLAN.md §10).

## Example (planned API shape, not yet real)

```rust
let mut doc = Document::new();
let artboard = doc.add_artboard("Main");
let node = doc.add_node(artboard, Node::rect(100.0, 100.0));
doc.apply(Command::SetTransform { node, transform }); // goes through undo stack
doc.undo();
```
