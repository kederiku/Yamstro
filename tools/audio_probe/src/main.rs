//! Banc d'essai de TASK-95 : les trois critères de la tâche 0 de l'Étape 8, mesurés sur
//! `bevy_seedling`. Ce paquet est détaché du workspace et ne lui survit pas : ce qui reste,
//! c'est `docs/adr/ADR-006-addendum.md`.
//!
//! `--measure` rend le son hors ligne, sans périphérique, et imprime les valeurs de l'addendum.
//! `--listen` joue sur la sortie audio, une touche par critère, pour l'oreille.

mod graph;
mod listen;
#[cfg(not(target_arch = "wasm32"))]
mod measure;
#[cfg(not(target_arch = "wasm32"))]
mod offline;
#[cfg(not(target_arch = "wasm32"))]
mod score;
mod sounds;
#[cfg(not(target_arch = "wasm32"))]
mod wav;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--measure") => measure::run(),
        Some("--listen") => listen::run(),
        _ => eprintln!("usage : audio_probe --measure | --listen"),
    }
}

/// Sur le Web, seule la voie d'écoute existe : elle sert à peser le backend (addendum, ligne WASM).
#[cfg(target_arch = "wasm32")]
fn main() {
    listen::run();
}
