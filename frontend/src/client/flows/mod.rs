//! Script du concepteur de flux, découpé comme les panneaux Leptos.

mod canvas;
mod grid;
mod simulation;

pub fn script() -> String {
    let mut source = String::with_capacity(24_000);
    source.push_str(grid::SCRIPT);
    source.push_str(canvas::SCRIPT);
    source.push_str(simulation::SCRIPT);
    source
}
