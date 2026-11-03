//! The Tiger's layered shell-damage model, the owner's "FEA damage" (docs/FEA_DAMAGE_MODEL_20261102.md).
//!
//! The calibrated ballistic law (`ballistics.rs`) stays the outcome authority for the struck plate.
//! This module follows the round through the layers behind it (`chain`), measures the vehicle's
//! components from the owner's CAD (`components`), and names every approximation it makes.

#![allow(dead_code)] // stages land before every consumer is wired; tests exercise them

pub mod chain;
pub mod components;
pub mod compartment;
pub mod fragments;
pub mod ignition;
pub mod lattice;
pub mod lodged;
pub mod materials;
pub mod section;
