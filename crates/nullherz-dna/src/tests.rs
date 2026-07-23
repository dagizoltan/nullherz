use std::sync::Arc;
use std::collections::HashMap;

use parking_lot::Mutex;
use crate::*;


#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_lineage_consensus_verification() {
        use ed25519_dalek::{SigningKey, Signer};
        let mut csprng = rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        let dna = nullherz_traits::SoundDNA::default();
        let dna_bytes = serde_json::to_vec(&dna).unwrap_or_default();
        let signature = signing_key.sign(&dna_bytes);

        let mut signed = SignedSoundDna {
            dna,
            signature: signature.to_bytes(),
            signer_public_key: verifying_key.to_bytes(),
            cas_id: None,
            parent_hashes: vec![[1u8; 32]],
            authorship_chain: vec!["alice".to_string()],
            generation: 1,
        };
        // Should succeed for a properly signed lineage record
        assert!(GeneticLineageConsensus::verify_lineage(&signed));

        // Should fail if signature is invalid
        signed.signature[0] ^= 1;
        assert!(!GeneticLineageConsensus::verify_lineage(&signed));
        signed.signature[0] ^= 1; // restore

        // Should fail if generation is 0 but has parents
        signed.generation = 0;
        assert!(!GeneticLineageConsensus::verify_lineage(&signed));

        // Should fail if generation is >0 but authorship_chain is empty
        signed.generation = 1;
        signed.authorship_chain = vec![];
        assert!(!GeneticLineageConsensus::verify_lineage(&signed));
    }

    #[test]
    fn test_library_database_crud() {
        let db_path = "test_library.redb";
        let _ = fs::remove_file(db_path);

        let db = LibraryDatabase::load(db_path).unwrap();

        let track = LibraryTrack {
            id: 1,
            path: "/test/path.wav".to_string(),
            title: "Test Track".to_string(),
            artist: "Test Artist".to_string(),
            album: "Test Album".to_string(),
            genre: "Test Genre".to_string(),
            energy_level: 0.8,
            metadata: Arc::new(nullherz_traits::SampleMetadata::new_empty()),
        };

        db.save_track(&track).unwrap();

        let loaded = db.get_track(1).unwrap().unwrap();
        assert_eq!(loaded, track);

        let list = db.list_tracks().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], track);

        let _ = fs::remove_file(db_path);
    }

    /// The in-memory facet index must track save / update / remove / reload so
    /// index-backed queries (query_tracks, smart crates) stay correct without a
    /// full-library redb scan.
    #[test]
    fn test_facet_index_coherence() {
        let db_path = format!("test_facet_coherence_{}.redb", std::process::id());
        let _ = fs::remove_file(&db_path);
        let db = LibraryDatabase::load(&db_path).unwrap();

        let mk = |id: u64, genre: &str, bpm: f32| {
            let mut m = nullherz_traits::SampleMetadata::new_empty();
            m.bpm = bpm;
            LibraryTrack {
                id, path: format!("/p/{id}.wav"), title: format!("t{id}"),
                artist: "a".into(), album: "al".into(), genre: genre.into(),
                energy_level: 0.5, metadata: Arc::new(m),
            }
        };

        db.save_track(&mk(1, "techno", 130.0)).unwrap();
        db.save_track(&mk(2, "house", 124.0)).unwrap();
        db.save_track(&mk(3, "techno", 128.0)).unwrap();

        // Genre query hits the index and fetches full tracks for matches only.
        let techno = db.query_tracks(Some("techno"), None, None, None).unwrap();
        assert_eq!(techno.len(), 2);
        assert!(techno.iter().all(|t| t.genre == "techno"));

        // BPM range.
        let fast = db.query_tracks(None, Some(129.0), None, None).unwrap();
        assert_eq!(fast.len(), 1);
        assert_eq!(fast[0].id, 1);

        // UPDATE: re-saving id 3 as house must move it in the index.
        db.save_track(&mk(3, "house", 128.0)).unwrap();
        assert_eq!(db.query_tracks(Some("techno"), None, None, None).unwrap().len(), 1);
        assert_eq!(db.query_tracks(Some("house"), None, None, None).unwrap().len(), 2);

        // REMOVE: id 1 must vanish from index queries.
        db.remove_track(1).unwrap();
        assert_eq!(db.query_tracks(Some("techno"), None, None, None).unwrap().len(), 0);
        assert_eq!(db.query_tracks(None, None, None, None).unwrap().len(), 2);

        // RELOAD: index rebuilt from disk agrees with redb.
        drop(db);
        let db2 = LibraryDatabase::load(&db_path).unwrap();
        assert_eq!(db2.query_tracks(Some("house"), None, None, None).unwrap().len(), 2);
        assert_eq!(db2.list_tracks().unwrap().len(), 2);

        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn test_sync_with_cloud_persistence() {
        let db_path = "test_sync.redb";
        let _ = std::fs::remove_file(db_path);
        let db = LibraryDatabase::load(db_path).unwrap();

        struct MockSync;
        impl PeerSync for MockSync {
            fn announce_dna(&self, _dna: &nullherz_traits::SoundDNA) {}
            fn request_dna(&self, id: u64) -> Option<nullherz_traits::SoundDNA> {
                if id == 0xABC { Some(nullherz_traits::SoundDNA::default()) } else { None }
            }
            fn list_peer_dna(&self) -> Vec<(u64, String)> {
                vec![(0xABC, "Cloud Track".to_string())]
            }
            fn gossip_metadata(&self, _known_ids: &[(u64, String)]) {}
            fn get_public_key(&self) -> [u8; 32] { [0u8; 32] }
        }

        db.sync_with_cloud(&MockSync).unwrap();

        let track = db.get_track(0xABC).unwrap().expect("Track should be persisted after sync");
        assert_eq!(track.title, "Cloud Track");
        assert_eq!(track.artist, "Cloud Peer");
        assert!(track.path.contains("cloud://"));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_library_crating() {
        let db_path = "test_crates.redb";
        let _ = fs::remove_file(db_path);
        let db = LibraryDatabase::load(db_path).unwrap();

        let track = LibraryTrack {
            id: 101,
            path: "/test/track.wav".to_string(),
            title: "Crate Track".to_string(),
            artist: "Crate Artist".to_string(),
            album: "Crate Album".to_string(),
            genre: "Techno".to_string(),
            energy_level: 0.9,
            metadata: Arc::new(nullherz_traits::SampleMetadata::new_empty()),
        };

        db.save_track(&track).unwrap();
        db.add_to_crate("Techno", 101).unwrap();

        let in_crate = db.get_tracks_in_crate("Techno").unwrap();
        assert_eq!(in_crate.len(), 1);
        assert_eq!(in_crate[0].id, 101);

        let empty_crate = db.get_tracks_in_crate("House").unwrap();
        assert!(empty_crate.is_empty());

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn test_handshake_pins_public_key_not_private() {
        use std::net::TcpListener;
        use ed25519_dalek::SigningKey;

        let actual_port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let db_path = format!("test_handshake_{}.redb", actual_port);
        let _ = fs::remove_file(&db_path);
        let lib = Arc::new(Mutex::new(LibraryDatabase::load(&db_path).unwrap()));

        let mut csprng = rand::rngs::OsRng;
        let sk = SigningKey::generate(&mut csprng);
        let sk_bytes = sk.to_bytes();
        let expected_pk = sk.verifying_key().to_bytes();

        DnaServer::start(lib, actual_port, Some(sk_bytes)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let peer = format!("127.0.0.1:{}", actual_port);
        let sync = CloudPeerSync {
            peers: vec![peer.clone()],
            trusted_peers: std::collections::HashSet::from([peer.clone()]),
            peer_keys: Mutex::new(HashMap::new()),
            signing_key: None,
            mesh_links: Mutex::new(std::collections::HashSet::new()),
        };

        // First contact pins the server's PUBLIC key — and must never be the
        // private signing key (regression: IDENTITY used to leak the secret).
        let pinned = sync.handshake(&peer).expect("handshake should succeed");
        assert_eq!(pinned, expected_pk);
        assert_ne!(pinned, sk_bytes, "IDENTITY must not expose the private signing key");
        assert_eq!(sync.peer_keys.lock().get(&peer).copied(), Some(expected_pk));

        // A peer whose identity key changes after pinning is rejected.
        sync.peer_keys.lock().insert(peer.clone(), [7u8; 32]);
        assert!(sync.handshake(&peer).is_none(), "key change after pinning must be rejected");

        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn test_genetic_similarity() {
        use nullherz_traits::SoundDNA;
        let mut dna_a = SoundDNA::default();
        let mut dna_b = SoundDNA::default();

        for i in 0..16 {
            dna_a.spectral.latent_space[i] = 0.5;
            dna_b.spectral.latent_space[i] = 0.55;
        }

        let sim = calculate_similarity(&dna_a, &dna_b);
        assert!(sim > 0.9);

        for i in 0..16 { dna_b.spectral.latent_space[i] = 1.0; }
        let sim_low = calculate_similarity(&dna_a, &dna_b);
        assert!(sim_low < sim);
    }

    #[test]
    fn test_simd_dna_interpolation() {
        use nullherz_traits::SoundDNA;
        let mut dna_a = SoundDNA::default();
        let mut dna_b = SoundDNA::default();

        for i in 0..16 {
            dna_a.spectral.latent_space[i] = 0.2;
            dna_b.spectral.latent_space[i] = 0.8;
        }

        let child = transfuse_dna(&dna_a, &dna_b, 0.5);

        for i in 0..16 {
            // Neural shaping applies tanh() to the result: 0.5.tanh() ~= 0.4621
            assert!((child.spectral.latent_space[i] - 0.5_f32.tanh()).abs() < 0.001);
        }

        let child_025 = transfuse_dna(&dna_a, &dna_b, 0.25);
        for i in 0..16 {
            // (0.2 * 0.75) + (0.8 * 0.25) = 0.15 + 0.2 = 0.35
            // 0.35.tanh() ~= 0.3363
            assert!((child_025.spectral.latent_space[i] - 0.35_f32.tanh()).abs() < 0.001);
        }
    }

    #[test]
    fn test_smart_crate_filtering() {
    }

    #[test]
    fn test_chaotic_transfusion_determinism() {
        use nullherz_traits::SoundDNA;
        let dna_a = SoundDNA::default();
        let dna_b = SoundDNA::default();

        let child_1 = chaotic_transfuse_dna(&dna_a, &dna_b, 0.5, 0.5);
        let child_2 = chaotic_transfuse_dna(&dna_a, &dna_b, 0.5, 0.5);

        assert_eq!(child_1, child_2);
    }

    #[test]
    fn test_chaotic_transfusion_mutation() {
        use nullherz_traits::SoundDNA;
        let dna_a = SoundDNA::default();
        let dna_b = SoundDNA::default();

        let normal = transfuse_dna(&dna_a, &dna_b, 0.5);
        let chaotic = chaotic_transfuse_dna(&dna_a, &dna_b, 0.5, 1.0);

        // Chaotic transfusion should produce different latent spaces due to mutations
        assert_ne!(normal.spectral.latent_space, chaotic.spectral.latent_space);
        assert!(chaotic.artifacts.glitch_density > normal.artifacts.glitch_density);
    }

    #[test]
    fn test_gossip_signature_enforcement_server() {
        use std::net::{TcpStream, TcpListener, SocketAddr};
        use std::io::{Write, Read, BufRead, BufReader};
        use ed25519_dalek::{SigningKey, Signer};
        use socket2::{Socket, Domain, Type, Protocol};

        let actual_port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let client_port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };

        let db_path = format!("test_gossip_srv_{}.redb", actual_port);
        let _ = fs::remove_file(&db_path);
        let lib = Arc::new(Mutex::new(LibraryDatabase::load(&db_path).unwrap()));

        let mut csprng = rand::rngs::OsRng;
        let sk = SigningKey::generate(&mut csprng);
        let sk_bytes = sk.to_bytes();

        DnaServer::start(lib.clone(), actual_port, Some(sk_bytes)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let server_addr: SocketAddr = format!("127.0.0.1:{}", actual_port).parse().unwrap();
        let client_addr: SocketAddr = format!("127.0.0.1:{}", client_port).parse().unwrap();

        // 1. Create client listener to accept the server's connect_timeout back on client_port
        let client_listener_sock = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
        client_listener_sock.set_reuse_address(true).unwrap();
        #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
        let _ = client_listener_sock.set_reuse_port(true);
        client_listener_sock.bind(&client_addr.into()).unwrap();
        client_listener_sock.listen(128).unwrap();
        let client_listener: TcpListener = client_listener_sock.into();

        // Spawn mock client DNA provider loop in background. It serves GET DNA
        // requests and answers HANDSHAKE with a consistent identity key — the
        // server's pull path pins peer identities and rejects DNA whose signer
        // does not match the pinned key.
        let client_listener_clone = client_listener.try_clone().unwrap();
        let mock_provider_thread = std::thread::spawn(move || {
            use ed25519_dalek::{SigningKey, Signer};
            let mut csprng = rand::rngs::OsRng;
            let client_sk = SigningKey::generate(&mut csprng);

            client_listener_clone.set_nonblocking(true).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            let mut served_get = false;
            let mut served_handshake = false;
            while std::time::Instant::now() < deadline && !(served_get && served_handshake) {
                let (mut incoming_stream, _) = match client_listener_clone.accept() {
                    Ok(conn) => conn,
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    }
                };
                incoming_stream.set_nonblocking(false).unwrap();
                let mut reader = BufReader::new(&incoming_stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok() {
                    let line_trimmed = line.trim();
                    if line_trimmed.starts_with("HANDSHAKE") {
                        let pk_hex = hex::encode(client_sk.verifying_key().to_bytes());
                        let _ = incoming_stream.write_all(format!("IDENTITY {}\n", pk_hex).as_bytes());
                        served_handshake = true;
                    } else if line_trimmed.starts_with("GET DNA ") {
                        // Sign and return a valid SignedSoundDna for track 2
                        let dna = nullherz_traits::SoundDNA::default();
                        let dna_bytes = serde_json::to_vec(&dna).unwrap_or_default();
                        let sig = client_sk.sign(&dna_bytes);

                        let signed = SignedSoundDna {
                            dna,
                            signature: sig.to_bytes(),
                            signer_public_key: client_sk.verifying_key().to_bytes(),
                            cas_id: None,
                            parent_hashes: Vec::new(),
                            authorship_chain: vec!["origin".to_string()],
                            generation: 1,
                        };
                        let _ = serde_json::to_writer(&mut incoming_stream, &signed);
                        served_get = true;
                    }
                }
            }
        });

        // 1. Send unsigned GOSSIP (does not need to bind to client_addr since server rejects unsigned gossip immediately)
        {
            let client_sock = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
            client_sock.connect(&server_addr.into()).unwrap();
            let mut stream: TcpStream = client_sock.into();

            let metadata = vec![(1u64, "Unsigned Track".to_string())];
            let payload = serde_json::to_string(&metadata).unwrap();
            let msg = format!("GOSSIP {}\n", payload);
            stream.write_all(msg.as_bytes()).unwrap();
            stream.flush().unwrap();
            // Connection will be closed by server
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).unwrap();
        }

        // Give any background task a moment
        std::thread::sleep(std::time::Duration::from_millis(100));
        {
            let db = lib.lock();
            let track = db.get_track(1).unwrap();
            assert!(track.is_none(), "Unsigned Track (ID 1) should NOT be present in database");
        }

        // 2. Send GOSSIP_SIGNED
        {
            let client_sock = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
            client_sock.set_reuse_address(true).unwrap();
            #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
            let _ = client_sock.set_reuse_port(true);
            client_sock.bind(&client_addr.into()).unwrap();
            client_sock.connect(&server_addr.into()).unwrap();
            let mut stream: TcpStream = client_sock.into();

            let metadata = vec![(2u64, "Signed Track".to_string())];
            let payload = serde_json::to_string(&metadata).unwrap();
            let sig = sk.sign(payload.as_bytes());
            let msg = format!(
                "GOSSIP_SIGNED {} {} {}\n",
                hex::encode(sig.to_bytes()),
                hex::encode(sk.verifying_key().to_bytes()),
                payload
            );
            stream.write_all(msg.as_bytes()).unwrap();
            stream.flush().unwrap();
            // Connection will be closed by server
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).unwrap();
        }

        // Wait for mock DNA provider to finish and background sync thread to save to the database
        let _ = mock_provider_thread.join();
        std::thread::sleep(std::time::Duration::from_millis(250));

        {
            let db = lib.lock();
            let track = db.get_track(2).unwrap();
            assert!(track.is_some(), "Signed Track (ID 2) should be present in database after signed GOSSIP");
            let t = track.unwrap();
            assert_eq!(t.title, "Signed Track");
        }

        let _ = fs::remove_file(&db_path);
    }
}
