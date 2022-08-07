use std::collections::HashMap;
use std::collections::hash_map::Entry::{Vacant, Occupied};
use std::fs::File;
use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize, Debug)]
pub struct StringToId {
    map: HashMap<String, u64>,
    next_id: u64,

    #[serde(default)]
    reverse_map: HashMap<u64, String>,
}

impl StringToId {
    pub fn new() -> StringToId {
        StringToId {
            map: HashMap::new(),
            next_id: 0,
            reverse_map: HashMap::new(),
        }
    }

    pub fn load(file_name: String) -> StringToId {
        let file = File::open(file_name).unwrap();
        let mut result: StringToId = super::serde_json::from_reader(file).unwrap();
        for (k, v) in result.map.iter() {
            result.reverse_map.insert(*v, k.to_string());
        }
        return result;
    }

    pub fn get(&self, key: String) -> u64 {
        *(self.map.get(&key).unwrap())
    }

    pub fn get_string(&self, id: u64) -> String {
        match self.reverse_map.get(&id) {
            Some(v) => v.to_string(),
            None => id.to_string(),
        }
    }

    pub fn keys(&self) -> Vec<String> {
        self.map.keys().into_iter().map(|e| e.to_string()).collect()
    }
}
