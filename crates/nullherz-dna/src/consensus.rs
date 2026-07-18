
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SignedSoundDna {
    pub dna: nullherz_traits::SoundDNA,
    #[serde(with = "serde_big_array::BigArray")]
    pub signature: [u8; 64],
    pub signer_public_key: [u8; 32],
    /// Content-Addressable Identifier (Blake3 hash of the serialized DNA)
    pub cas_id: Option<[u8; 32]>,

    // LINEAGE CONSENSUS EXTENSIONS
    #[serde(default)]
    pub parent_hashes: Vec<[u8; 32]>,
    #[serde(default)]
    pub authorship_chain: Vec<String>,
    #[serde(default)]
    pub generation: u32,
}

/// Cryptographic and lineage-based consensus verifier.
pub struct GeneticLineageConsensus;

impl GeneticLineageConsensus {
    pub fn verify_signature(signed_dna: &SignedSoundDna) -> bool {
        use ed25519_dalek::{Verifier, Signature, VerifyingKey};
        let pub_key_res = VerifyingKey::from_bytes(&signed_dna.signer_public_key);
        let sig_res = Signature::from_slice(&signed_dna.signature);

        if let (Ok(pub_key), Ok(sig)) = (pub_key_res, sig_res) {
            let dna_bytes = serde_json::to_vec(&signed_dna.dna).unwrap_or_default();
            pub_key.verify(&dna_bytes, &sig).is_ok()
        } else {
            false
        }
    }

    pub fn verify_lineage(signed_dna: &SignedSoundDna) -> bool {
        if !Self::verify_signature(signed_dna) {
            return false;
        }
        // Height check: if parents exist, generation must be > 0.
        if !signed_dna.parent_hashes.is_empty() && signed_dna.generation == 0 {
            return false;
        }
        // Authorship check: active ancestry requires at least one author registered.
        if signed_dna.generation > 0 && signed_dna.authorship_chain.is_empty() {
            return false;
        }
        true
    }
}

pub(crate) mod serde_arc {
    use std::sync::Arc;
    use serde::{Serialize, Deserialize, Serializer, Deserializer};

    pub fn serialize<T, S>(val: &Arc<T>, s: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        val.as_ref().serialize(s)
    }

    pub fn deserialize<'de, T, D>(d: D) -> Result<Arc<T>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        T::deserialize(d).map(Arc::new)
    }
}

