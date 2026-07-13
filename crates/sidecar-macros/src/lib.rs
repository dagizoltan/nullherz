use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemStruct};

#[proc_macro_attribute]
pub fn sidecar(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let name = &input.ident;

    let expanded = quote! {
        #input

        impl #name {
            pub fn run_as_sidecar(processor: impl audio_core::AudioProcessor + 'static) {
                let args: Vec<String> = std::env::args().collect();
                if args.len() < 5 {
                    eprintln!("Usage: sidecar --command-shm <name> --channels <n> --signal-shm <name> --event-fd <fd> [--input-shm <name> ...]");
                    return;
                }

                // Hardened: structured argument parsing for SHM and EventFD
                let mut cmd_shm = "";
                let mut sig_shm = "";
                let mut efd_val = -1;
                let mut channels = 2;

                let mut i = 0;
                while i < args.len() {
                    match args[i].as_str() {
                        "--command-shm" => { if let Some(val) = args.get(i + 1) { cmd_shm = val; i += 1; } }
                        "--signal-shm" => { if let Some(val) = args.get(i + 1) { sig_shm = val; i += 1; } }
                        "--event-fd" => { if let Some(val) = args.get(i + 1) { efd_val = val.parse().unwrap_or(-1); i += 1; } }
                        "--channels" => { if let Some(val) = args.get(i + 1) { channels = val.parse().unwrap_or(2); i += 1; } }
                        _ => {}
                    }
                    i += 1;
                }

                println!("Sidecar {} started with {} channels", stringify!(#name), channels);

                // Map SHM segments and run the sidecar loop using sidecar-sdk
                let mut inputs = Vec::new();
                let mut outputs = Vec::new();
                for i in 0..args.len() {
                    if args[i] == "--input-shm" && i + 1 < args.len() { inputs.push(args[i+1].clone()); }
                    if args[i] == "--output-shm" && i + 1 < args.len() { outputs.push(args[i+1].clone()); }
                }

                unsafe {
                    let _ = ipc_layer::pin_thread_to_core(1); // Pin sidecars to core 1 by default
                    let mut sidecar = sidecar_sdk::SidecarHost::new(cmd_shm, sig_shm, &inputs, &outputs, efd_val);
                    sidecar.run(processor);
                }
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro]
pub fn sidecar_builder(_item: TokenStream) -> TokenStream {
    let expanded = quote! {
        pub struct SidecarApp;
        impl SidecarApp {
            pub fn build_and_run(name: &str, processor: impl audio_core::AudioProcessor + 'static) {
                println!("Building sidecar: {}", name);
                // Implementation mirrors 'run_as_sidecar' but provided as a standalone builder
                let args: Vec<String> = std::env::args().collect();
                // Hardened: structured argument parsing
                let cmd_shm = args.iter().position(|a| a == "--command-shm")
                    .and_then(|i| args.get(i + 1)).map(|s| s.as_str()).unwrap_or("");
                let sig_shm = args.iter().position(|a| a == "--signal-shm")
                    .and_then(|i| args.get(i + 1)).map(|s| s.as_str()).unwrap_or("");
                let efd_val = args.iter().position(|a| a == "--event-fd")
                    .and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(-1);
                let channels = args.iter().position(|a| a == "--channels")
                    .and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(2);

                let mut inputs = Vec::new();
                let mut outputs = Vec::new();
                let sc_names = Vec::new(); // Sidechains empty by default for sidecar_builder
                for i in 0..args.len() {
                    if args[i] == "--input-shm" && i + 1 < args.len() { inputs.push(args[i+1].clone()); }
                    if args[i] == "--output-shm" && i + 1 < args.len() { outputs.push(args[i+1].clone()); }
                }

                unsafe {
                    let _ = ipc_layer::pin_thread_to_core(1);
                    let mut sidecar = sidecar_sdk::SidecarHost::new(cmd_shm, sig_shm, &inputs, &sc_names, &outputs, efd_val);
                    sidecar.run(processor);
                }
            }
        }
    };
    TokenStream::from(expanded)
}
