use nullherz_conductor::modulation_matrix::ModulationMatrix;
use nullherz_traits::{Command, MixerCommand};

#[test]
fn test_modulation_matrix_expansion() {
    let mut matrix = ModulationMatrix::new();
    matrix.add_mapping(0, 100, 1, 2.0, 0);
    matrix.add_mapping(0, 101, 2, 0.5, 0);

    let commands = matrix.expand_macro(0, 0.8);
    assert_eq!(commands.len(), 1); // Grouped into 1 bundle

    match &commands[0] {
        Command::Mixer(nullherz_traits::MixerCommand::Bundle { count, .. }) => {
            assert_eq!(*count, 2);
            let sub_cmds: Vec<_> = commands[0].bundle_iter().unwrap().collect();
            assert_eq!(sub_cmds.len(), 2);

            if let Command::Mixer(MixerCommand::SetParam { target_id, param_id, value, .. }) = sub_cmds[0] {
                assert_eq!(target_id, 100);
                assert_eq!(param_id, 1);
                assert!((value - 1.6).abs() < 1e-6);
            } else { panic!("Expected SetParam in bundle"); }

            if let Command::Mixer(MixerCommand::SetParam { target_id, param_id, value, .. }) = sub_cmds[1] {
                assert_eq!(target_id, 101);
                assert_eq!(param_id, 2);
                assert!((value - 0.4).abs() < 1e-6);
            } else { panic!("Expected SetParam in bundle"); }
        }
        _ => panic!("Expected Bundle command"),
    }
}
