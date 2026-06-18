use bit_vec::BitVec;
use serde::{
    de::{self, SeqAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Serialize,
};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// A standard BloomFilter supporting serde serialization, Clone, and Debug.
#[derive(Debug, Clone)]
pub struct BloomFilter {
    bits: BitVec,
    num_hashes: u32,
    /// The exact number of bits (BitVec::from_bytes may round up)
    exact_num_bits: usize,
}

impl Serialize for BloomFilter {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("BloomFilter", 3)?;
        let bytes = self.bits.to_bytes();
        state.serialize_field("bits", &bytes)?;
        state.serialize_field("num_hashes", &self.num_hashes)?;
        state.serialize_field("exact_num_bits", &self.exact_num_bits)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for BloomFilter {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[allow(dead_code)]
        enum Field { Bits, NumHashes, ExactNumBits }
        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Field, D::Error> {
                struct FieldVisitor;
                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;
                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("`bits`, `num_hashes`, or `exact_num_bits`")
                    }
                    fn visit_str<E: de::Error>(self, value: &str) -> Result<Field, E> {
                        match value {
                            "bits" => Ok(Field::Bits),
                            "num_hashes" => Ok(Field::NumHashes),
                            "exact_num_bits" => Ok(Field::ExactNumBits),
                            _ => Err(de::Error::unknown_field(value, &["bits", "num_hashes", "exact_num_bits"])),
                        }
                    }
                }
                deserializer.deserialize_identifier(FieldVisitor)
            }
        }
        struct BloomFilterVisitor;
        impl<'de> Visitor<'de> for BloomFilterVisitor {
            type Value = BloomFilter;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct BloomFilter")
            }
            fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<BloomFilter, V::Error> {
                let bits: Vec<u8> = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let num_hashes: u32 = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let exact_num_bits: usize = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;

                // BitVec::from_bytes creates len = bits.len() * 8, which may be larger
                // than exact_num_bits. We need to truncate to the exact bit count.
                let mut bv = BitVec::from_bytes(&bits);
                if bv.len() > exact_num_bits {
                    bv.truncate(exact_num_bits);
                }

                Ok(BloomFilter {
                    bits: bv,
                    num_hashes,
                    exact_num_bits,
                })
            }
        }
        const FIELDS: &[&str] = &["bits", "num_hashes", "exact_num_bits"];
        deserializer.deserialize_struct("BloomFilter", FIELDS, BloomFilterVisitor)
    }
}

impl BloomFilter {
    /// Create a new BloomFilter with the specified number of bits and hashes
    pub fn with_size(num_bits: usize, num_hashes: u32) -> Self {
        Self {
            bits: BitVec::from_elem(num_bits, false),
            num_hashes,
            exact_num_bits: num_bits,
        }
    }

    /// Create a BloomFilter that expects to hold `expected_num_items`.
    /// The filter will be sized to have a false positive rate of `rate`.
    pub fn with_rate(rate: f32, expected_num_items: u32) -> Self {
        let bits = needed_bits(rate, expected_num_items);
        Self::with_size(bits, optimal_num_hashes(bits, expected_num_items))
    }

    /// Insert item into this BloomFilter.
    /// Returns `true` if the item was already present (probabilistically).
    pub fn insert<T: Hash>(&mut self, item: &T) -> bool {
        let hashes = self.compute_hashes(item);
        let mut contained = true;
        for h in hashes {
            let idx = (h % self.bits.len() as u64) as usize;
            match self.bits.get(idx) {
                Some(b) => {
                    if !b {
                        contained = false;
                    }
                }
                None => panic!("Hash mod failed in insert"),
            }
            self.bits.set(idx, true);
        }
        !contained
    }

    /// Check if the item has been inserted into this bloom filter.
    /// Can return false positives, but not false negatives.
    pub fn contains<T: Hash>(&self, item: &T) -> bool {
        let hashes = self.compute_hashes(item);
        for h in hashes {
            let idx = (h % self.bits.len() as u64) as usize;
            match self.bits.get(idx) {
                Some(b) => {
                    if !b {
                        return false;
                    }
                }
                None => panic!("Hash mod failed"),
            }
        }
        true
    }

    /// Remove all values from this BloomFilter
    pub fn clear(&mut self) {
        self.bits.clear();
        self.exact_num_bits = 0;
    }

    /// Get the number of bits
    pub fn num_bits(&self) -> usize {
        self.bits.len()
    }

    /// Get the number of hash functions
    pub fn num_hashes(&self) -> u32 {
        self.num_hashes
    }

    fn compute_hashes<T: Hash>(&self, item: &T) -> Vec<u64> {
        let mut h1 = DefaultHasher::new();
        item.hash(&mut h1);
        let hash1 = h1.finish();

        let mut h2 = DefaultHasher::new();
        hash1.hash(&mut h2);
        item.hash(&mut h2);
        let hash2 = h2.finish();

        (0..self.num_hashes)
            .map(|i| hash1.wrapping_add(i as u64).wrapping_mul(hash2))
            .collect()
    }
}

/// Return the optimal number of hashes to use for the given number of bits and items
pub fn optimal_num_hashes(num_bits: usize, num_items: u32) -> u32 {
    let num_bits_f = num_bits as f32;
    let num_items_f = num_items as f32;
    let hashes = (num_bits_f / num_items_f * std::f32::consts::LN_2).round() as u32;
    hashes.max(2).min(200)
}

/// Return the number of bits needed to satisfy the specified false positive rate
pub fn needed_bits(false_pos_rate: f32, num_items: u32) -> usize {
    let ln22 = std::f32::consts::LN_2 * std::f32::consts::LN_2;
    (num_items as f32 * ((1.0 / false_pos_rate).ln() / ln22)).round() as usize
}