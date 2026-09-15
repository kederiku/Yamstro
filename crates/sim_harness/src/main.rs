//! Le binaire du harnais. Tout vit dans la bibliothèque, pour que le fixture
//! d'accord de TASK-155 puisse l'atteindre depuis une autre crate.

fn main() -> std::process::ExitCode {
    sim_harness::run()
}
