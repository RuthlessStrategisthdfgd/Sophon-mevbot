use crate::vrng::VRNG;
use vrf::openssl::{CipherSuite, ECVRF};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use rand_core::RngCore;
use parity_wordlist::WORDS;
use std::fmt::{Display};
use vrf::VRF;
use rand::seq::SliceRandom;
use secp256k1::{SecretKey};

#[derive(Debug)]
pub enum InvalidVVRF{
    InvalidProofError, 
    InvalidPubKeyError, 
    InvalidMessageError,
}

impl Display for InvalidVVRF {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidVVRF::InvalidProofError => write!(f, "Invalid proof"),
            InvalidVVRF::InvalidPubKeyError => write!(f, "Invalid public key"),
            InvalidVVRF::InvalidMessageError => write!(f, "Invalid message"),
        }
    
    }
}

impl std::error::Error for InvalidVVRF {}

///VVRF type contains all params necessary for creating and verifying an rng 
///It does not include the secret key 
pub struct VVRF {
    pub vrf: ECVRF,
    pub pubkey: Vec<u8>,
    pub message: Vec<u8>,
    pub proof: [u8; 81],
    pub hash: [u8; 32],
    pub rng: ChaCha20Rng,
}

///implenent VRNG trait for VVRF s.t. VVRF can accomomdate 
impl VRNG for VVRF {
    fn generate_u8(&mut self) -> u8 {
        let mut data = [0u8; 1];
        self.rng.fill_bytes(&mut data);
        u8::from_be_bytes(data)
    }

    fn generate_u16(&mut self) -> u16 {
        let mut data = [0u8; 2];
        self.rng.fill_bytes(&mut data);
        u16::from_be_bytes(data)
    }

    fn generate_u32(&mut self) -> u32 {
        let mut data = [0u8; 4];
        self.rng.fill_bytes(&mut data);
        u32::from_be_bytes(data)
    }

    fn generate_u64(&mut self) -> u64 {
        let mut data = [0u8; 8];
        self.rng.fill_bytes(&mut data);
        u64::from_be_bytes(data)
    }

    fn generate_u128(&mut self) -> u128 {
        let mut data = [0u8; 16];
        self.rng.fill_bytes(&mut data);
        u128::from_be_bytes(data)
    }

    fn generate_usize(&mut self) -> usize {
        let data = &[0u8; 8];
        let (int_bytes, _) = data.split_at(std::mem::size_of::<usize>());
        usize::from_be_bytes(int_bytes.try_into().unwrap())
    }

    fn generate_u8_in_range(&mut self, min: u8, max: u8) -> u8{
        let mut data = [0u8; 1];
        self.rng.fill_bytes(&mut data);
        let num = u8::from_be_bytes(data) % (max-min+1) +min;
        return num % (max - min +1) + min;
    }

    fn generate_u16_in_range(&mut self, min: u16, max: u16) -> u16{
        let mut data = [0u8; 2];
        self.rng.fill_bytes(&mut data);
        let num = u16::from_be_bytes(data) % (max-min+1) +min;
        return num % (max - min +1) + min;
    }

    fn generate_u32_in_range(&mut self, min: u32, max: u32) -> u32{
        let mut data = [0u8; 4];
        self.rng.fill_bytes(&mut data);
        let num = u32::from_be_bytes(data) % (max-min+1) +min;
        return num % (max - min +1) + min;
    }

    fn generate_u64_in_range(&mut self, min: u64, max: u64) -> u64{
        let mut data = [0u8; 8];
        self.rng.fill_bytes(&mut data);
        let num = u64::from_be_bytes(data) % (max-min+1) +min;
        return num % (max - min +1) + min;
    }

    fn generate_u128_in_range(&mut self, min: u128, max: u128) -> u128{
        let mut data = [0u8; 16];
        self.rng.fill_bytes(&mut data);
        let num = u128::from_be_bytes(data) % (max-min+1) +min;
        return num % (max - min +1) + min;
    }

    fn generate_usize_in_range(&mut self, min: usize, max: usize) -> usize{
        let data = &[0u8; 8];
        let (int_bytes, _) = data.split_at(std::mem::size_of::<usize>());
        let num = usize::from_be_bytes(int_bytes.try_into().unwrap());
        return num % (max - min +1) + min;
    }

    fn generate_word(&mut self) -> String {
        let mut rng = self.rng.clone();
        WORDS.choose(&mut rng).unwrap().trim_start().to_string()
    }

    fn generate_words(&mut self, n: usize) -> Vec<String> {
        let mut rng = self.rng.clone();
        (0..n).map(|_| WORDS.choose(&mut rng).unwrap().to_string()).collect::<Vec<_>>()
    }

    fn generate_phrase(&mut self, n: usize) -> String {
        let mut rng = self.rng.clone();
        (0..n).map(|_| WORDS.choose(&mut rng).unwrap()).fold(String::new(), |mut acc, word| {
            acc.push_str(" ");
            acc.push_str(word);
            acc
        }).trim_start().to_string()
    }
}


///implement VVRF type by passing a secretKey such that
///all the VVRF fields can now be calculated thanks to the sk being passed
///and use of fxns defined below and imported
impl VVRF {
    ///create new VVRF type by populating fields with return types
    /// of VVRF methods
    pub fn new(message: &[u8], sk: SecretKey) -> VVRF {
        let mut vrf = VVRF::generate_vrf(CipherSuite::SECP256K1_SHA256_TAI);
        let pubkey = match VVRF::generate_pubkey(&mut vrf, sk){
            Ok(p) => Ok(p),
            Err(e) => Err(e),
        };
    
        let (proof, hash) = VVRF::generate_seed(&mut vrf, message, sk).unwrap();
        let rng = ChaCha20Rng::from_seed(hash);
        VVRF {
            vrf,
            pubkey: pubkey.unwrap(),
            message: message.to_vec(),
            proof,
            hash,
            rng: rng,
        }
    }

    pub fn generate_secret_key() -> SecretKey {
        return SecretKey::new(&mut rand::thread_rng());
    }

    ///get vrf from openssl struct ECVRF (eliptic curve vrf)
    fn generate_vrf(suite: CipherSuite) -> ECVRF {
        ECVRF::from_suite(suite).unwrap()
    }


    ///get pk from vrf crate
    fn generate_pubkey(vrf: &mut ECVRF, secret_key: SecretKey) -> Result<Vec<u8>, InvalidVVRF> {
        match vrf.derive_public_key(&secret_key.secret_bytes()) {
            Ok(pk) => Ok(pk),
            Err(_) => Err(crate::vvrf::InvalidVVRF::InvalidPubKeyError),
        }
    }

    ///generate seed
    fn generate_seed(
        vrf: &mut ECVRF,
        message: &[u8],
        secret_key: SecretKey,
    ) -> Option<([u8; 81], [u8; 32])> {
        if let Ok(pi) = vrf.prove(&secret_key.secret_bytes(), message) {
            if let Ok(hash) = vrf.proof_to_hash(&pi) {
                let mut proof_buff = [0u8; 81];
                pi.iter().enumerate().for_each(|(i, v)| {
                    proof_buff[i] = *v;
                });
                let mut hash_buff = [0u8; 32];
                hash.iter().enumerate().for_each(|(i, v)| {
                    hash_buff[i] = *v;
                });

                Some((proof_buff, hash_buff))
            } else {
                None
            }
        } else {
            None
        }
    }

    ///check that hash and beta are equal to ensure hash(seed) is valid
    pub fn verify_seed(&mut self) -> Result<(), InvalidVVRF> {
        if let Ok(beta) = self.vrf.verify(&self.pubkey, &self.proof, &self.message) {
            if self.hash.to_vec() != beta {
                return Err(InvalidVVRF::InvalidProofError);
            } else {
                return Ok(());
            }
        } else {
            return Err(InvalidVVRF::InvalidProofError); 
        };
    }

    ///getter functions
    pub fn get_pubkey(&self) -> Vec<u8> {
        self.pubkey.clone()
    }

    pub fn get_message(&self) -> Vec<u8> {
        self.message.clone()
    }

    pub fn get_proof(&self) -> [u8; 81] {
        self.proof
    }

    pub fn get_hash(&self) -> [u8; 32] {
        self.hash
    }

    pub fn get_rng(&self) -> ChaCha20Rng {
        self.rng.clone()
    }
}
