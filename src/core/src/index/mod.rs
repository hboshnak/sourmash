//! # Indexing structures for fast similarity search
//!
//! An index organizes signatures to allow for fast similarity search.
//! Some indices also support containment searches.

pub mod bigsi;
pub mod linear;
pub mod sbt;

pub mod storage;

pub mod search;

use std::ops::Deref;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use atomic_float::AtomicF64;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::index::sbt::{Node, SBT};
use crate::index::search::{search_minhashes, search_minhashes_containment};
use crate::index::storage::{ReadData, ReadDataError, Storage};
use crate::signature::{Signature, SigsTrait};
use crate::sketch::nodegraph::Nodegraph;
use crate::sketch::Sketch;
use crate::Error;

pub type MHBT = SBT<Node<Nodegraph>, Signature>;

/* FIXME: bring back after MQF works on macOS and Windows
use cfg_if::cfg_if;
cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
      use mqf::MQF;
      pub type MHMT = SBT<Node<MQF>, Signature>;
    }
}
*/

pub trait Index<'a> {
    type Item: Comparable<Self::Item>;
    //type SignatureIterator: Iterator<Item = Self::Item>;

    fn find<F>(
        &self,
        search_fn: F,
        sig: &Self::Item,
        threshold: f64,
    ) -> Result<Vec<&Self::Item>, Error>
    where
        F: Fn(&dyn Comparable<Self::Item>, &Self::Item, f64) -> bool,
    {
        Ok(self
            .signature_refs()
            .into_iter()
            .flat_map(|node| {
                if search_fn(&node, sig, threshold) {
                    Some(node)
                } else {
                    None
                }
            })
            .collect())
    }

    fn search(
        &self,
        sig: &Self::Item,
        threshold: f64,
        containment: bool,
    ) -> Result<Vec<&Self::Item>, Error> {
        if containment {
            self.find(search_minhashes_containment, sig, threshold)
        } else {
            self.find(search_minhashes, sig, threshold)
        }
    }

    //fn gather(&self, sig: &Self::Item, threshold: f64) -> Result<Vec<&Self::Item>, Error>;

    fn insert(&mut self, node: Self::Item) -> Result<(), Error>;

    fn batch_insert(&mut self, nodes: Vec<Self::Item>) -> Result<(), Error> {
        for node in nodes {
            self.insert(node)?;
        }

        Ok(())
    }

    fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Error>;

    fn load<P: AsRef<Path>>(path: P) -> Result<(), Error>;

    fn signatures(&self) -> Vec<Self::Item>;

    fn signature_refs(&self) -> Vec<&Self::Item>;

    /*
    fn iter_signatures(&self) -> Self::SignatureIterator;
    */
}

// TODO: split into two traits, Similarity and Containment?
pub trait Comparable<O> {
    fn similarity(&self, other: &O) -> f64;
    fn containment(&self, other: &O) -> f64;
}

impl<'a, N, L> Comparable<L> for &'a N
where
    N: Comparable<L>,
{
    fn similarity(&self, other: &L) -> f64 {
        (*self).similarity(&other)
    }

    fn containment(&self, other: &L) -> f64 {
        (*self).containment(&other)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DatasetInfo {
    pub filename: String,
    pub name: String,
    pub metadata: String,
}

#[derive(TypedBuilder, Default, Clone)]
pub struct SigStore<T> {
    #[builder(setter(into))]
    filename: String,

    #[builder(setter(into))]
    name: String,

    #[builder(setter(into))]
    metadata: String,

    storage: Option<Rc<dyn Storage>>,

    #[builder(setter(into), default)]
    data: OnceCell<T>,
}

impl<T> SigStore<T> {
    pub fn name(&self) -> String {
        self.name.clone()
    }
}

impl<T> std::fmt::Debug for SigStore<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SigStore [filename: {}, name: {}, metadata: {}]",
            self.filename, self.name, self.metadata
        )
    }
}

impl ReadData<Signature> for SigStore<Signature> {
    fn data(&self) -> Result<&Signature, Error> {
        if let Some(sig) = self.data.get() {
            Ok(sig)
        } else if let Some(storage) = &self.storage {
            let sig = self.data.get_or_init(|| {
                let raw = storage.load(&self.filename).unwrap();
                let sigs: Result<Vec<Signature>, _> = serde_json::from_reader(&mut &raw[..]);
                if let Ok(sigs) = sigs {
                    // TODO: select the right sig?
                    sigs[0].to_owned()
                } else {
                    let sig: Signature = serde_json::from_reader(&mut &raw[..]).unwrap();
                    sig
                }
            });

            Ok(sig)
        } else {
            Err(ReadDataError::LoadError.into())
        }
    }
}

impl SigStore<Signature> {
    pub fn count_common(&self, other: &SigStore<Signature>) -> u64 {
        let ng: &Signature = self.data().unwrap();
        let ong: &Signature = other.data().unwrap();

        // TODO: select the right signatures...
        // TODO: better matching here, what if it is not a mh?
        if let Sketch::MinHash(mh) = &ng.signatures[0] {
            if let Sketch::MinHash(omh) = &ong.signatures[0] {
                return mh.count_common(&omh, false).unwrap() as u64;
            }
        }
        unimplemented!();
    }

    pub fn mins(&self) -> Vec<u64> {
        let ng: &Signature = self.data().unwrap();

        // TODO: select the right signatures...
        // TODO: better matching here, what if it is not a mh?
        if let Sketch::MinHash(mh) = &ng.signatures[0] {
            mh.mins()
        } else {
            unimplemented!()
        }
    }
}

impl From<SigStore<Signature>> for Signature {
    fn from(other: SigStore<Signature>) -> Signature {
        other.data.get().unwrap().to_owned()
    }
}

impl Deref for SigStore<Signature> {
    type Target = Signature;

    fn deref(&self) -> &Signature {
        self.data.get().unwrap()
    }
}

impl From<Signature> for SigStore<Signature> {
    fn from(other: Signature) -> SigStore<Signature> {
        let name = other.name();
        let filename = other.filename();

        SigStore::builder()
            .name(name)
            .filename(filename)
            .data(other)
            .metadata("")
            .storage(None)
            .build()
    }
}

impl Comparable<SigStore<Signature>> for SigStore<Signature> {
    fn similarity(&self, other: &SigStore<Signature>) -> f64 {
        let ng: &Signature = self.data().unwrap();
        let ong: &Signature = other.data().unwrap();

        // TODO: select the right signatures...
        // TODO: better matching here, what if it is not a mh?
        if let Sketch::MinHash(mh) = &ng.signatures[0] {
            if let Sketch::MinHash(omh) = &ong.signatures[0] {
                return mh.similarity(&omh, true, false).unwrap();
            }
        }

        /* FIXME: bring back after boomphf changes
        if let Sketch::UKHS(mh) = &ng.signatures[0] {
            if let Sketch::UKHS(omh) = &ong.signatures[0] {
                return 1. - mh.distance(&omh);
            }
        }
        */

        unimplemented!()
    }

    fn containment(&self, other: &SigStore<Signature>) -> f64 {
        let ng: &Signature = self.data().unwrap();
        let ong: &Signature = other.data().unwrap();

        // TODO: select the right signatures...
        // TODO: better matching here, what if it is not a mh?
        if let Sketch::MinHash(mh) = &ng.signatures[0] {
            if let Sketch::MinHash(omh) = &ong.signatures[0] {
                let common = mh.count_common(&omh, false).unwrap();
                let size = mh.size();
                return common as f64 / size as f64;
            }
        }
        unimplemented!()
    }
}

impl Comparable<Signature> for Signature {
    fn similarity(&self, other: &Signature) -> f64 {
        // TODO: select the right signatures...
        // TODO: better matching here, what if it is not a mh?
        if let Sketch::MinHash(mh) = &self.signatures[0] {
            if let Sketch::MinHash(omh) = &other.signatures[0] {
                return mh.similarity(&omh, true, false).unwrap();
            }
        }

        /* FIXME: bring back after boomphf changes
        if let Sketch::UKHS(mh) = &self.signatures[0] {
            if let Sketch::UKHS(omh) = &other.signatures[0] {
                return 1. - mh.distance(&omh);
            }
        }
        */

        unimplemented!()
    }

    fn containment(&self, other: &Signature) -> f64 {
        // TODO: select the right signatures...
        // TODO: better matching here, what if it is not a mh?
        if let Sketch::MinHash(mh) = &self.signatures[0] {
            if let Sketch::MinHash(omh) = &other.signatures[0] {
                let common = mh.count_common(&omh, false).unwrap();
                let size = mh.size();
                return common as f64 / size as f64;
            }
        }
        unimplemented!()
    }
}

impl<L> From<DatasetInfo> for SigStore<L> {
    fn from(other: DatasetInfo) -> SigStore<L> {
        SigStore {
            filename: other.filename,
            name: other.name,
            metadata: other.metadata,
            storage: None,
            data: OnceCell::new(),
        }
    }
}

#[repr(u32)]
pub enum SearchType {
    Jaccard = 1,
    Containment = 2,
    MaxContainment = 3,
}

pub struct JaccardSearch {
    search_type: SearchType,
    threshold: AtomicF64,
    require_scaled: bool,
    best_only: bool,
}

impl JaccardSearch {
    pub fn new(search_type: SearchType) -> Self {
        let require_scaled = match search_type {
            SearchType::Containment | SearchType::MaxContainment => true,
            SearchType::Jaccard => false,
        };

        JaccardSearch {
            search_type,
            threshold: AtomicF64::new(0.0),
            require_scaled,
            best_only: false,
        }
    }

    pub fn with_threshold(search_type: SearchType, threshold: f64) -> Self {
        let s = Self::new(search_type);
        s.set_threshold(threshold);
        s
    }

    pub fn set_best_only(&mut self, best_only: bool) {
        self.best_only = best_only;
    }

    pub fn set_threshold(&self, threshold: f64) {
        self.threshold
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |_| Some(threshold))
            .unwrap();
    }

    pub fn check_is_compatible(&self, sig: &Signature) -> Result<(), Error> {
        // TODO: implement properly
        Ok(())
    }

    pub fn score(
        &self,
        query_size: u64,
        shared_size: u64,
        subject_size: u64,
        total_size: u64,
    ) -> f64 {
        let shared_size = shared_size as f64;
        match self.search_type {
            SearchType::Jaccard => shared_size / total_size as f64,
            SearchType::Containment => {
                if query_size == 0 {
                    0.0
                } else {
                    shared_size / query_size as f64
                }
            }
            SearchType::MaxContainment => {
                let min_denom = query_size.min(subject_size);
                if min_denom == 0 {
                    0.0
                } else {
                    shared_size / min_denom as f64
                }
            }
        }
    }

    /// Return True if this match should be collected.
    pub fn collect(&self, score: f64, subj: &Signature) -> bool {
        if self.best_only {
            self.threshold.fetch_max(score, Ordering::Relaxed);
        }
        true
    }

    /// Return true if this score meets or exceeds the threshold.
    ///
    /// Note: this can be used whenever a score or estimate is available
    /// (e.g. internal nodes on an SBT). `collect(...)`, below, decides
    /// whether a particular signature should be collected, and/or can
    /// update the threshold (used for BestOnly behavior).
    pub fn passes(&self, score: f64) -> bool {
        score > 0. && score >= self.threshold.load(Ordering::SeqCst)
    }
}
