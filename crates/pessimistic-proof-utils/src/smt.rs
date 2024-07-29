use std::collections::HashMap;
use std::hash::Hash;

use pessimistic_proof::local_exit_tree::hasher::Hasher;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use thiserror::Error;

use crate::utils::empty_hash_at_height;

/// A trait for types that can be converted to a fixed-size array of bits.
pub trait ToBits<const NUM_BITS: usize> {
    fn to_bits(&self) -> [bool; NUM_BITS];
}

#[derive(Error, Debug, Eq, PartialEq)]
pub(crate) enum SmtError {
    #[error("trying to insert a key already in the SMT")]
    KeyAlreadyPresent,
    #[error("trying to generate a Merkle proof for a key not in the SMT")]
    KeyNotPresent,
    #[error("trying to generate a non-inclusion proof for a key present in the SMT")]
    KeyPresent,
}

/// A node in an SMT.
#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct Node<H>
where
    H: Hasher,
    H::Digest: Serialize + DeserializeOwned,
{
    #[serde_as(as = "_")]
    left: H::Digest,
    #[serde_as(as = "_")]
    right: H::Digest,
}

impl<H> Clone for Node<H>
where
    H: Hasher,
    H::Digest: Clone + Serialize + DeserializeOwned,
{
    fn clone(&self) -> Self {
        Node {
            left: self.left.clone(),
            right: self.right.clone(),
        }
    }
}

impl<H> Copy for Node<H>
where
    H: Hasher,
    H::Digest: Copy + Serialize + DeserializeOwned,
{
}

impl<H> Node<H>
where
    H: Hasher,
    H::Digest: Serialize + DeserializeOwned,
{
    pub fn hash(&self) -> H::Digest {
        H::merge(&self.left, &self.right)
    }
}

/// An in-memory sparse merkle tree (SMT) consistent with a zero-initialized
/// Merkle tree.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Smt<H, const DEPTH: usize>
where
    H: Hasher,
    H::Digest: Copy + Eq + Hash + Serialize + DeserializeOwned,
{
    /// The SMT root.
    #[serde_as(as = "_")]
    root: H::Digest,
    /// A map from node hash to node.
    #[serde_as(as = "HashMap<_, _>")]
    tree: HashMap<H::Digest, Node<H>>,
    /// `empty_hash_at_height[i]` is the root of an empty Merkle tree of depth
    /// `i`.
    #[serde_as(as = "[_; DEPTH]")]
    empty_hash_at_height: [H::Digest; DEPTH],
}

/// An inclusion proof for a key in an SMT.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SmtInclusionProof<H, const DEPTH: usize>
where
    H: Hasher,
    H::Digest: Copy + Eq + Hash + Serialize + DeserializeOwned,
{
    #[serde_as(as = "[_; DEPTH]")]
    siblings: [H::Digest; DEPTH],
}

/// A non-inclusion proof for a key in an SMT.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SmtNonInclusionProof<H, const DEPTH: usize>
where
    H: Hasher,
    H::Digest: Copy + Eq + Hash + Serialize + DeserializeOwned,
{
    #[serde_as(as = "Vec<_>")]
    siblings: Vec<H::Digest>,
}

impl<H, const DEPTH: usize> Default for Smt<H, DEPTH>
where
    H: Hasher,
    H::Digest: Copy + Eq + Hash + Serialize + DeserializeOwned + Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<H, const DEPTH: usize> Smt<H, DEPTH>
where
    H: Hasher,
    H::Digest: Copy + Eq + Hash + Serialize + DeserializeOwned,
{
    /// Constructs a new, empty `Smt`.
    pub fn new() -> Self
    where
        H::Digest: Default,
    {
        let empty_hash_at_height = empty_hash_at_height::<H, DEPTH>();
        let root = H::merge(
            &empty_hash_at_height[DEPTH - 1],
            &empty_hash_at_height[DEPTH - 1],
        );
        let tree = HashMap::new();
        Smt {
            root,
            tree,
            empty_hash_at_height,
        }
    }

    /// Returns the value associated with the given key, if any.
    pub fn get<K>(&self, key: K) -> Option<H::Digest>
    where
        K: ToBits<DEPTH>,
    {
        let mut hash = self.root;
        for b in key.to_bits() {
            hash = if b {
                self.tree.get(&hash)?.right
            } else {
                self.tree.get(&hash)?.left
            };
        }

        Some(hash)
    }

    fn insert_helper(
        &mut self,
        hash: H::Digest,
        depth: usize,
        bits: &[bool; DEPTH],
        value: H::Digest,
    ) -> Result<H::Digest, SmtError> {
        if depth == DEPTH {
            return if hash != self.empty_hash_at_height[0] {
                Err(SmtError::KeyAlreadyPresent)
            } else {
                Ok(value)
            };
        }
        let node = self.tree.get(&hash);
        assert!(depth < DEPTH, "`depth` should be less than `DEPTH`");
        let mut node = node.copied().unwrap_or(Node {
            left: self.empty_hash_at_height[DEPTH - depth - 1],
            right: self.empty_hash_at_height[DEPTH - depth - 1],
        });
        let node_place = if bits[depth] {
            &mut node.right
        } else {
            &mut node.left
        };
        *node_place = self.insert_helper(*node_place, depth + 1, bits, value)?;

        let new_hash = node.hash();
        self.tree.insert(new_hash, node);

        Ok(new_hash)
    }

    /// Inserts a key-value pair into the SMT.
    /// Returns an error if the key is already in the SMT.
    pub fn insert<K>(&mut self, key: K, value: H::Digest) -> Result<(), SmtError>
    where
        K: ToBits<DEPTH>,
    {
        self.root = self.insert_helper(self.root, 0, &key.to_bits(), value)?;

        Ok(())
    }

    /// Returns an inclusion proof for the given key.
    /// Returns an error if the key is not in the SMT.
    pub fn get_inclusion_proof<K>(&self, key: K) -> Result<SmtInclusionProof<H, DEPTH>, SmtError>
    where
        K: ToBits<DEPTH>,
    {
        let mut siblings = [self.empty_hash_at_height[0]; DEPTH];
        let mut hash = self.root;
        let bits = key.to_bits();
        for i in 0..DEPTH {
            let node = self.tree.get(&hash).ok_or(SmtError::KeyNotPresent)?;
            siblings[DEPTH - i - 1] = if bits[i] { node.left } else { node.right };
            hash = if bits[i] { node.right } else { node.left };
        }
        if hash == self.empty_hash_at_height[0] {
            return Err(SmtError::KeyNotPresent);
        }

        Ok(SmtInclusionProof { siblings })
    }

    /// Returns a non-inclusion proof for the given key.
    /// Returns an error if the key is in the SMT.
    pub fn get_non_inclusion_proof<K>(
        &self,
        key: K,
    ) -> Result<SmtNonInclusionProof<H, DEPTH>, SmtError>
    where
        K: ToBits<DEPTH>,
    {
        let mut siblings = vec![];
        let mut hash = self.root;
        let bits = key.to_bits();
        for i in 0..DEPTH {
            if self.empty_hash_at_height.contains(&hash) {
                return Ok(SmtNonInclusionProof { siblings });
            }
            let node = self.tree.get(&hash);
            let node = match node {
                Some(node) => node,
                None => {
                    debug_assert!(
                        hash == H::merge(
                            &self.empty_hash_at_height[DEPTH - i - 1],
                            &self.empty_hash_at_height[DEPTH - i - 1]
                        ),
                        "The SMT is messed up"
                    );
                    return Ok(SmtNonInclusionProof { siblings });
                }
            };
            siblings.push(if bits[i] { node.left } else { node.right });
            hash = if bits[i] { node.right } else { node.left };
        }
        if hash != self.empty_hash_at_height[0] {
            return Err(SmtError::KeyPresent);
        }

        Ok(SmtNonInclusionProof { siblings })
    }
}

impl<H, const DEPTH: usize> SmtInclusionProof<H, DEPTH>
where
    H: Hasher,
    H::Digest: Copy + Eq + Hash + Serialize + DeserializeOwned,
{
    /// Returns `true` if and only if the proof is valid for the given key,
    /// value, and root.
    pub fn verify<K>(&self, key: K, value: H::Digest, root: H::Digest) -> bool
    where
        K: ToBits<DEPTH>,
    {
        let bits = key.to_bits();
        let mut hash = value;
        for i in 0..DEPTH {
            hash = if bits[DEPTH - i - 1] {
                H::merge(&self.siblings[i], &hash)
            } else {
                H::merge(&hash, &self.siblings[i])
            };
        }

        hash == root
    }
}

impl<H, const DEPTH: usize> SmtNonInclusionProof<H, DEPTH>
where
    H: Hasher,
    H::Digest: Copy + Eq + Hash + Serialize + DeserializeOwned,
{
    /// Returns `true` if and only if the proof is valid for the given key and
    /// root.
    pub fn verify<K>(
        &self,
        key: K,
        root: H::Digest,
        empty_hash_at_height: &[H::Digest; DEPTH],
    ) -> bool
    where
        K: ToBits<DEPTH>,
    {
        if self.siblings.len() > DEPTH {
            return false;
        }
        if self.siblings.is_empty() {
            return root
                == H::merge(
                    &empty_hash_at_height[DEPTH - 1],
                    &empty_hash_at_height[DEPTH - 1],
                );
        }
        let bits = key.to_bits();
        let mut entry = empty_hash_at_height[DEPTH - self.siblings.len()];
        for i in (0..self.siblings.len()).rev() {
            let sibling = self.siblings[i];
            entry = if bits[i] {
                H::merge(&sibling, &entry)
            } else {
                H::merge(&entry, &sibling)
            };
        }

        entry == root
    }

    /// Verify the non-inclusion proof (i.e. that `key` is not in the SMT) and
    /// return the updated root of the SMT with `(key, value)` inserted.
    pub fn verify_and_update<K>(
        &self,
        key: K,
        value: H::Digest,
        root: H::Digest,
        empty_hash_at_height: &[H::Digest; DEPTH],
    ) -> Option<H::Digest>
    where
        K: Copy + ToBits<DEPTH>,
    {
        if !self.verify(key, root, empty_hash_at_height) {
            return None;
        }

        let mut entry = value;
        let bits = key.to_bits();
        for i in (self.siblings.len()..DEPTH).rev() {
            let sibling = empty_hash_at_height[DEPTH - i - 1];
            entry = if bits[i] {
                H::merge(&sibling, &entry)
            } else {
                H::merge(&entry, &sibling)
            };
        }
        for i in (0..self.siblings.len()).rev() {
            let sibling = self.siblings[i];
            entry = if bits[i] {
                H::merge(&sibling, &entry)
            } else {
                H::merge(&entry, &sibling)
            };
        }

        Some(entry)
    }
}

impl ToBits<32> for u32 {
    fn to_bits(&self) -> [bool; 32] {
        std::array::from_fn(|i| (self >> i) & 1 == 1)
    }
}

#[cfg(test)]
mod tests {
    use std::hash::Hash;

    use pessimistic_proof::local_exit_tree::hasher::Keccak256Hasher;
    use rand::prelude::SliceRandom;
    use rand::{random, thread_rng, Rng};
    use rs_merkle::{Hasher as MerkleHasher, MerkleTree};
    use tiny_keccak::{Hasher as _, Keccak};

    use crate::smt::{Smt, SmtError, ToBits};

    const DEPTH: usize = 32;
    type H = Keccak256Hasher;

    impl ToBits<8> for u8 {
        fn to_bits(&self) -> [bool; 8] {
            std::array::from_fn(|i| (self >> i) & 1 == 1)
        }
    }

    #[derive(Clone, Debug)]
    pub struct TestKeccak256;

    impl MerkleHasher for TestKeccak256 {
        type Hash = [u8; 32];

        fn hash(data: &[u8]) -> [u8; 32] {
            let mut keccak256 = Keccak::v256();
            keccak256.update(data);
            let mut output = [0u8; 32];
            keccak256.finalize(&mut output);
            output
        }
    }

    fn check_no_duplicates<A: Eq + Hash, B>(v: &[(A, B)]) {
        let mut seen = std::collections::HashSet::new();
        for (a, _) in v {
            assert!(seen.insert(a), "Duplicate key. Check your rng.");
        }
    }

    #[test]
    fn test_compare_with_other_impl() {
        const DEPTH: usize = 8;
        let mut rng = thread_rng();
        let num_keys = rng.gen_range(0..=1 << DEPTH);
        let mut smt = Smt::<H, DEPTH>::new();
        let mut kvs: Vec<_> = (0..u8::MAX).map(|i| (i, random())).collect();
        kvs.shuffle(&mut rng);
        for (key, value) in &kvs[..num_keys] {
            smt.insert(*key, *value).unwrap();
        }

        let mut leaves = vec![[0_u8; 32]; 1 << DEPTH];
        for (key, value) in &kvs[..num_keys] {
            leaves[key.reverse_bits() as usize] = *value;
        }
        let mt: MerkleTree<TestKeccak256> = MerkleTree::from_leaves(&leaves);

        assert_eq!(smt.root, mt.root().unwrap());
    }

    #[test]
    fn test_order_consistency() {
        let mut rng = thread_rng();
        let num_keys = rng.gen_range(0..100);
        let mut smt = Smt::<H, DEPTH>::new();
        let mut kvs: Vec<(u32, _)> = (0..num_keys).map(|_| (random(), random())).collect();
        check_no_duplicates(&kvs);
        for (key, value) in kvs.iter() {
            smt.insert(*key, *value).unwrap();
        }
        let mut shuffled_smt = Smt::<H, DEPTH>::new();
        kvs.shuffle(&mut rng);
        for (key, value) in kvs.iter() {
            shuffled_smt.insert(*key, *value).unwrap();
        }

        assert_eq!(smt.root, shuffled_smt.root);
    }

    #[test]
    fn test_inclusion_proof() {
        let mut rng = thread_rng();
        let num_keys = rng.gen_range(1..100);
        let mut smt = Smt::<H, DEPTH>::new();
        let kvs: Vec<(u32, _)> = (0..num_keys).map(|_| (random(), random())).collect();
        check_no_duplicates(&kvs);
        for (key, value) in kvs.iter() {
            smt.insert(*key, *value).unwrap();
        }
        let (key, value) = *kvs.choose(&mut rng).unwrap();
        let proof = smt.get_inclusion_proof(key).unwrap();
        assert!(proof.verify(key, value, smt.root));
    }

    #[test]
    fn test_inclusion_proof_wrong_value() {
        let mut rng = thread_rng();
        let num_keys = rng.gen_range(1..100);
        let mut smt = Smt::<H, DEPTH>::new();
        let kvs: Vec<(u32, _)> = (0..num_keys).map(|_| (random(), random())).collect();
        check_no_duplicates(&kvs);
        for (key, value) in kvs.iter() {
            smt.insert(*key, *value).unwrap();
        }
        let (key, real_value) = *kvs.choose(&mut rng).unwrap();
        let proof = smt.get_inclusion_proof(key).unwrap();
        let fake_value = random();
        assert_ne!(real_value, fake_value, "Check your rng");
        assert!(!proof.verify(key, fake_value, smt.root));
    }

    #[test]
    fn test_non_inclusion_proof() {
        let mut rng = thread_rng();
        let num_keys = rng.gen_range(0..100);
        let mut smt = Smt::<H, DEPTH>::new();
        let kvs: Vec<(u32, _)> = (0..num_keys).map(|_| (random(), random())).collect();
        check_no_duplicates(&kvs);
        for (key, value) in kvs.iter() {
            smt.insert(*key, *value).unwrap();
        }
        let key: u32 = random();
        assert!(
            kvs.iter().position(|(k, _)| k == &key).is_none(),
            "Check your rng"
        );
        let proof = smt.get_non_inclusion_proof(key).unwrap();
        assert!(proof.verify(key, smt.root, &smt.empty_hash_at_height));
    }

    #[test]
    fn test_non_inclusion_proof_failing() {
        let mut rng = thread_rng();
        let num_keys = rng.gen_range(1..100);
        let mut smt = Smt::<H, DEPTH>::new();
        let kvs: Vec<(u32, _)> = (0..num_keys).map(|_| (random(), random())).collect();
        check_no_duplicates(&kvs);
        for (key, value) in kvs.iter() {
            smt.insert(*key, *value).unwrap();
        }
        let (key, _) = *kvs.choose(&mut rng).unwrap();
        let error = smt.get_non_inclusion_proof(key).unwrap_err();
        assert_eq!(error, SmtError::KeyPresent);
    }

    fn test_non_inclusion_proof_and_update(num_keys: usize) {
        let mut smt = Smt::<H, DEPTH>::new();
        let kvs: Vec<(u32, _)> = (0..num_keys).map(|_| (random(), random())).collect();
        check_no_duplicates(&kvs);
        for (key, value) in kvs.iter() {
            smt.insert(*key, *value).unwrap();
        }
        let key: u32 = random();
        assert!(
            kvs.iter().position(|(k, _)| k == &key).is_none(),
            "Check your rng"
        );
        let proof = smt.get_non_inclusion_proof(key).unwrap();
        assert!(proof.verify(key, smt.root, &smt.empty_hash_at_height));
        let value = random();
        let new_root = proof
            .verify_and_update(key, value, smt.root, &smt.empty_hash_at_height)
            .unwrap();
        smt.insert(key, value).unwrap();
        assert_eq!(smt.root, new_root);
    }

    #[test]
    fn test_non_inclusion_proof_and_update_empty() {
        test_non_inclusion_proof_and_update(0)
    }

    #[test]
    fn test_non_inclusion_proof_and_update_nonempty() {
        let num_keys = thread_rng().gen_range(1..100);
        test_non_inclusion_proof_and_update(num_keys)
    }
}
