#[cfg(test)]
mod tests {
    use crate::Conductor;
    use nullherz_traits::{PerformanceCommand, Command, MixerCommand, DeckParamType};
    use nullherz_dna::GeneticLibrary;

    #[tokio::test]
    async fn test_mixing_two_tracks() {
        let mut conductor = Conductor::with_library_path("test_mixing.redb");

        // 1. Bootstrap Mixer
        let mut mixer = nullherz_mixer::MixerManager::new();
        let bootstrap_commands = mixer.create_4channel_mixer();
        conductor.apply_mixer_commands(bootstrap_commands);

        // 2. Scan and Register tracks
        let tracks_path = if std::path::Path::new("tracks").exists() {
            "tracks"
        } else if std::path::Path::new("../../tracks").exists() {
            "../../tracks"
        } else {
            panic!("Could not find tracks directory in {} or ../../tracks", std::env::current_dir().unwrap().display());
        };

        if let Some(ref monitor) = conductor.folder_monitor {
            monitor.scan_folder(tracks_path);
        }

        // Wait a bit for registration to complete (it's synchronous in scan_folder)
        let tracks = conductor.library.lock().unwrap().list_tracks().unwrap();
        assert!(tracks.len() >= 2, "Expected at least 2 tracks, found {}", tracks.len());

        let track_a_id = tracks.iter().find(|t| t.title == "track_a.wav").unwrap().id;
        let track_b_id = tracks.iter().find(|t| t.title == "track_b.wav").unwrap().id;

        // 3. Load tracks to decks
        conductor.apply_mixer_commands(vec![
            Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: "A".chars().next().unwrap(), sample_id: track_a_id }),
            Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: "B".chars().next().unwrap(), sample_id: track_b_id }),
        ]);

        // 4. Play decks
        conductor.apply_mixer_commands(vec![
            Command::Performance(PerformanceCommand::PlayDeck { deck_id: "A".chars().next().unwrap() }),
            Command::Performance(PerformanceCommand::PlayDeck { deck_id: "B".chars().next().unwrap() }),
        ]);

        // 5. Mix! (Set Deck A gain to 0.8, Deck B to 0.2)
        conductor.apply_mixer_commands(vec![
            Command::Mixer(MixerCommand::SetDeckParam { deck_id: "A".chars().next().unwrap(), param_type: DeckParamType::Gain, value: 0.8 }),
            Command::Mixer(MixerCommand::SetDeckParam { deck_id: "B".chars().next().unwrap(), param_type: DeckParamType::Gain, value: 0.2 }),
        ]);

        // 6. Crossfade to center
        conductor.apply_mixer_commands(vec![
            Command::Mixer(MixerCommand::SetParam { target_id: 100, param_id: 0, value: 0.5, ramp_duration_samples: 0 }),
        ]);

        // Clean up
        let _ = std::fs::remove_file("test_mixing.redb");
    }
}
