use std::collections::{BTreeMap};

use std::collections::{HashMap};

use serde_json::{Value};
use serde::{Deserialize};

#[derive(Debug, Deserialize, Default)]
pub struct DataValue {
    pub value: Value,
    #[serde(rename = "type")]
    pub dtype: String,
}

#[derive(Debug, Deserialize)]
pub struct Snak {
    pub datatype: String,
    pub datavalue: DataValue,
}

#[derive(Debug, Deserialize)]
pub struct Claim {
    pub rank: String,
    pub mainsnak: Snak,
}

#[derive(Debug, Deserialize, Default)]
pub struct ClaimList {
    #[serde(default)]
    #[serde(rename = "P31")]
    pub p31: Vec<Claim>
}

#[derive(Debug, Deserialize)]
pub struct WikiLabel {
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct SiteLink {
    pub title: String,
}


#[derive(Debug, Deserialize)]
pub struct WikidataItem {
    pub id: String,
    #[serde(default)]
    pub labels: HashMap<String, WikiLabel>,
    #[serde(default)]
    pub aliases: HashMap<String, Vec<WikiLabel>>,
    #[serde(default)]
    pub sitelinks: HashMap<String, SiteLink>,
    #[serde(default)]
    pub claims: ClaimList,
}


pub struct WDDocument {
    pub wid: u64,
    pub rank: u64,
    pub instance_of: Vec<u64>, 
    pub add_to_index: bool,
    pub titles: Vec<(String, String)>,
    pub sitelinks: Vec<(String, String)>,
}

impl WDDocument {
    pub fn new(wid: u64, rank: u64, instance_of: Vec<u64>, add_to_index: bool) -> WDDocument {
        WDDocument {
            wid: wid,
            rank: rank,
            instance_of: instance_of,
            add_to_index: add_to_index,
            titles: Vec::new(),
            sitelinks: Vec::new(),
        }
    }
}

impl Clone for WDDocument {
    fn clone(&self) -> WDDocument {
        WDDocument {
            wid: self.wid,
            rank: self.rank,
            instance_of: self.instance_of.clone(),
            add_to_index: self.add_to_index.clone(),
            titles: self.titles.clone(),
            sitelinks: self.sitelinks.clone(),
        }
    }
}

#[inline]
fn update_mapping<T: Clone>(
    mapping: &mut BTreeMap<String, Vec<T>>,
    key: &str,
    obj: &T,
) {
    match mapping.get_mut(key) {
        Some(values) => {
            values.push(obj.clone());
        }
        None => {
            let mut values: Vec<T> = Vec::new();
            values.push(obj.clone());
            mapping.insert(key.to_string(), values);
        }
    }
}

pub struct WDDocumentPool {
    pub documents: BTreeMap<String, Vec<WDDocument>>,
    pub len: u64,
}

impl WDDocumentPool {
    pub fn new() -> WDDocumentPool {
        WDDocumentPool {
            documents: BTreeMap::new(),
            len: 0,
        }
    }

    pub fn clear(&mut self) {
        self.documents.clear();
        self.len = 0;
    }

    #[inline]
    pub fn insert(&mut self, key: String, wddocument: &WDDocument) {
        update_mapping(&mut self.documents, key.as_str(), &wddocument);
        self.len += 1;
    }

    /*

    fn iter(&self) -> WDDocumentPoolIterator<'static> {
        let f = self.labels.iter();
        let b = f.next().1.iter();
        return WDDocumentPoolIterator { document_pool: self, btree_iterator: f, hashset_iterator: b }
    } 
     */   
}

/*
struct WDDocumentPoolIterator<'a> {
    document_pool: &'a WDDocumentPool<'a>,
    btree_iterator: std::collections::btree_map::Iter<'a, String, HashSet<WDDocument>>,
    hashset_iterator: std::collections::hash_set::Iter<'a, WDDocument>,
}

impl<'a, WDDocument> Iterator for WDDocumentPoolIterator<'a> {
    type Item = &'a WDDocument;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.hashset_iterator.next();
        if result.is_some() {
            return result;
        }
        match self.btree_iterator.next() {
            Some((_,docs)) => {
                self.hashset_iterator = docs.iter();
                return self.hashset_iterator.next();
            }
            None => {
                return None;
            }
        }
    }
}
 */