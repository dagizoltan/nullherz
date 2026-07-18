
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

        // 2. Ensure test tracks exist and Scan
        let test_tracks_dir = "test_tracks_mixing";
        std::fs::create_dir_all(test_tracks_dir).unwrap();

        let track_a_path = format!("{}/track_a.wav", test_tracks_dir);
        let track_b_path = format!("{}/track_b.wav", test_tracks_dir);

        {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer_a = hound::WavWriter::create(&track_a_path, spec).unwrap();
            for t in 0..44100 {
                let val = (t as f32 * 440.0 * 2.0 * std::f32::consts::PI / 44100.0).sin();
                writer_a.write_sample((val * 32767.0) as i16).unwrap();
            }
            let mut writer_b = hound::WavWriter::create(&track_b_path, spec).unwrap();
            for t in 0..44100 {
                let val = if (t % 200) < 100 { 0.5f32 } else { -0.5f32 };
                writer_b.write_sample((val * 32767.0) as i16).unwrap();
            }
        }

        if let Some(ref monitor) = conductor.folder_monitor {
            monitor.scan_folder(test_tracks_dir);
        }

        // Wait a bit for registration to complete (asynchronous scan)
        let mut tracks = Vec::new();
        for _ in 0..100 {
            { let lib = conductor.library.lock();
                if let Ok(t) = lib.list_tracks() {
                    tracks = t;
                    if tracks.len() >= 2 {
                        break;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(tracks.len() >= 2, "Expected at least 2 tracks, found {}", tracks.len());

        let track_a = tracks.iter().find(|t| t.title == "track_a.wav").unwrap().clone();
        let track_b = tracks.iter().find(|t| t.title == "track_b.wav").unwrap().clone();

        // Setup root keys for harmonic sync testing (C and F)
        let mut meta_a = (*track_a.metadata).clone(); meta_a.root_key = Some(0.0); let mut track_a = track_a.clone(); track_a.metadata = std::sync::Arc::new(meta_a);
        let mut meta_b = (*track_b.metadata).clone(); meta_b.root_key = Some(5.0); let mut track_b = track_b.clone(); track_b.metadata = std::sync::Arc::new(meta_b);

        {
            let lib = conductor.library.lock();
            lib.save_track(&track_a).unwrap();
            lib.save_track(&track_b).unwrap();
        }

        // 3. Load tracks to decks
        conductor.apply_mixer_commands(vec![
            Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: track_a.id }),
            Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'B', sample_id: track_b.id }),
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
        let _ = std::fs::remove_dir_all(test_tracks_dir);
    }
}
