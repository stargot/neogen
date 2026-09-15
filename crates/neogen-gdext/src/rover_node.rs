//! `RoverNode` — visual mirror of a core rover (backlog 3.3).
//!
//! The node carries **no physics**: the authoritative position lives in
//! the core world; this is rendering glue only. Every render frame the
//! node chases the rover's current core position through the shared
//! coordinate rule ([`crate::coords`]).
//!
//! # Interpolation scheme
//!
//! `position = position.lerp(core_position, tick_alpha())` where
//! `tick_alpha` is SimNode's accumulator fraction toward the next tick
//! (classic render/tick decoupling). Right after a tick the factor is
//! small (fresh accumulator), growing to 1 as the next tick nears — the
//! node closes most of the gap just before the world state changes again.
//! The chase never overshoots and converges when the rover parks. Exact
//! between-tick interpolation (previous-tick snapshots) is deferred until
//! the core exposes them; the visual difference at 30 tps is negligible.

use godot::builtin::{Color, NodePath, Rect2, Vector2};
use godot::classes::{INode2D, Node2D};
use godot::prelude::*;

use crate::sim_node::SimNode;

/// Visual placeholder for one rover: a flat square that mirrors the core.
#[derive(GodotClass)]
#[class(base = Node2D)]
struct RoverNode {
    base: Base<Node2D>,
    /// Id of the mirrored rover (core `RoverId`).
    #[var]
    rover_id: i64,
    /// Path to the driving SimNode (resolved once on ready).
    #[var]
    sim_path: NodePath,
    sim: Option<Gd<SimNode>>,
}

#[godot_api]
impl INode2D for RoverNode {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            rover_id: 1,
            sim_path: NodePath::from("../Sim"),
            sim: None,
        }
    }

    fn ready(&mut self) {
        match self.base().get_node_or_null(&self.sim_path) {
            Some(node) => match node.try_cast::<SimNode>() {
                Ok(sim) => self.sim = Some(sim),
                Err(_) => {
                    godot_warn!(
                        "Neogen: node at {} is not a SimNode",
                        self.sim_path.to_string()
                    );
                }
            },
            None => {
                godot_warn!("Neogen: no SimNode at {}", self.sim_path.to_string());
            }
        }
        self.base_mut().queue_redraw();
    }

    fn process(&mut self, _delta: f64) {
        self.mirror_tick();
    }

    fn draw(&mut self) {
        // Placeholder body; the art pass is backlog phase 11.
        self.base_mut().draw_rect(
            Rect2::new(Vector2::new(-8.0, -8.0), Vector2::new(16.0, 16.0)),
            Color::from_rgb(0.29, 0.73, 0.45),
        );
    }
}

impl RoverNode {
    /// Chase the core position: pure visual smoothing, no state authority.
    fn mirror_tick(&mut self) {
        let (target, alpha) = match self.sim.as_mut() {
            Some(sim) => {
                let mut sim = sim.bind_mut();
                (
                    sim.get_rover_position(self.rover_id),
                    sim.tick_alpha().clamp(0.0, 1.0),
                )
            }
            None => return,
        };
        let next = self.base().get_position().lerp(target, alpha);
        self.base_mut().set_position(next);
    }
}
