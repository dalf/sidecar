use std::collections::HashMap;
use std::collections::hash_map::Entry::{Vacant, Occupied};

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct StringToId {
    map: HashMap<String, u64>,
    next_id: u64,
}

impl StringToId {
    pub fn new() -> StringToId {
        StringToId {
            map: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn get(&mut self, key: String) -> u64 {
        match self.map.entry(key) {
           Vacant(entry) => {
            self.next_id += 1;
            entry.insert(self.next_id);
            return self.next_id;
           },
           Occupied(entry) => {
            return *entry.get();
           }
        };
    }
}
