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

                // Simplified parsing for macro prototype
                let mut cmd_shm = "";
                let mut sig_shm = "";
                let mut efd_val = 0;
                let mut channels = 0;

                for i in 0..args.len() {
                    match args[i].as_str() {
                        "--command-shm" if i + 1 < args.len() => cmd_shm = &args[i+1],
                        "--signal-shm" if i + 1 < args.len() => sig_shm = &args[i+1],
                        "--event-fd" if i + 1 < args.len() => efd_val = args[i+1].parse().unwrap_or(0),
                        "--channels" if i + 1 < args.len() => channels = args[i+1].parse().unwrap_or(0),
                        _ => {}
                    }
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
                    let mut sidecar = sidecar_sdk::SidecarHost::new(cmd_shm, sig_shm, &inputs, &outputs, efd_val);
                    sidecar.run(processor);
                }
            }
        }
    };

    TokenStream::from(expanded)
}
