//! Témoin de la pesée WASM : la même application que la voie d'écoute, sans `bevy_seedling`.
//! La différence de taille entre `audio_probe` et ce binaire, après `wasm-opt -Oz`, est le
//! poids du backend audio (Firewheel, cpal et les décodeurs), consigné dans l'addendum.

use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};

fn main() {
    App::new()
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
            LogPlugin::default(),
            AssetPlugin {
                file_path: "assets".to_string(),
                ..Default::default()
            },
        ))
        .run();
}
