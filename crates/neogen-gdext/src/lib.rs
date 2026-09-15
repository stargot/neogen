//! GDExtension bridge between Godot 4.7 and the Neogen simulation core.
//!
//! This crate only glues: the simulation lives in `neogen-core`, the Lua
//! runtime in `neogen-script` — neither knows about Godot. Phase 3 grows
//! `SimNode` (3.2) and the rover/log bridges (3.3/3.4) on top of this
//! skeleton.

mod coords;
mod sim_node;

use godot::classes::INode;
use godot::classes::Node;
use godot::prelude::*;

/// Minimal extension class proving the bridge is alive (backlog 3.1).
#[derive(GodotClass)]
#[class(base = Node)]
struct NeogenBridge {
    base: Base<Node>,
}

#[godot_api]
impl INode for NeogenBridge {
    fn init(base: Base<Node>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl NeogenBridge {
    /// Smoke-test method callable from GDScript.
    #[func]
    fn hello(&self) -> GString {
        "Neogen bridge alive".into()
    }
}

struct NeogenExtension;

#[gdextension]
unsafe impl ExtensionLibrary for NeogenExtension {
    fn on_stage_init(stage: InitStage) {
        godot_print!("Neogen GDExtension loaded (godot-rust 0.5.5, Godot 4.7 API): {stage:?}");
    }
}
