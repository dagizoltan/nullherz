use nullherz_conductor::modulation_matrix::ModulationMatrix;
use nullherz_traits::{Command, MixerCommand};

#[test]
fn test_modulation_matrix_expansion() {
    let mut matrix = ModulationMatrix::new();
    matrix.add_mapping(0, 100, 1, 2.0, 0);
    matrix.add_mapping(0, 101, 2, 0.5, 0);

    let commands = matrix.expand_macro(0, 0.8);
    assert_eq!(commands.len(), 2);

    match commands[0] {
        Command::Mixer(MixerCommand::SetParam { target_id, param_id, value, .. }) => {
            assert_eq!(target_id, 100);
            assert_eq!(param_id, 1);
            assert!((value - 1.6).abs() < 1e-6);
        }
        _ => panic!("Expected SetParam command"),
    }

    match commands[1] {
        Command::Mixer(MixerCommand::SetParam { target_id, param_id, value, .. }) => {
            assert_eq!(target_id, 101);
            assert_eq!(param_id, 2);
            assert!((value - 0.4).abs() < 1e-6);
        }
        _ => panic!("Expected SetParam command"),
    }
}
