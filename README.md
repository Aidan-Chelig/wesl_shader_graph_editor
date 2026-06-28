# WESL Shader Graph Editor

A typed node shader editor for building portable WGSL/WESL shader graphs with a live Bevy preview.

## Features

- Visual shader graph editing with typed ports for `f32`, `vec2`, `vec3`, and `vec4` values.
- Generated WESL, linked WGSL, and Bevy preview shader views.
- Live preview on selectable primitives.
- Built-in math, vector, texture, and LYGIA-based utility nodes.
- Ambiguous output type resolution for polymorphic nodes such as multiply and LYGIA color operations.
- Connection validation that rejects edits which would break downstream type constraints.
- User-made module nodes with editable module tabs.
- Workspace node and global node categories in the right-click node finder.

## Module Nodes

Select a connected group of nodes, right-click, and choose `Create Module Node` to package the selection into a reusable module.

When a module is created, the selection is replaced by a small wrapper graph:

- cloned uniform nodes for the module inputs
- the new module node
- a new `Fragment Output` wired from the module output

Right-click a module node and choose `Edit Module` to open it in a tab below the toolbar. The preview and generated source follow the active tab, so editing a module previews that module graph directly.

Saved user modules are loaded into the `Global Nodes` category. Modules in the current project appear under `Workspace Nodes`.

## Development

Run the native app:

```bash
cargo run
```

Check the project:

```bash
cargo check
```

Run tests:

```bash
cargo test
```

## Notes

The editor stores shader graph projects as RON and generates shader code from the current graph. Global user modules are saved under the app's user module directory and loaded at startup.
