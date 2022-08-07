use std::collections::{HashMap, HashSet};

use serde::Deserialize;

extern crate serde;


#[derive(Debug, Deserialize, Clone)]
pub struct InstanceOfMapping {
    #[serde(rename = "instance_of")]
    map: HashMap<u64, String>,
    ignore: Vec<u64>,
    do_not_index: HashSet<u64>,
}

pub enum InstanceOfType<'a> {
    NotFound,
    Label(&'a String),
    IgnoreEntity,
}


impl InstanceOfMapping {
    pub fn new() -> InstanceOfMapping {
        InstanceOfMapping { map: HashMap::new(), ignore: Vec::new(), do_not_index: HashSet::new() }
    }

    pub fn load_from(file_name: &str) -> InstanceOfMapping {
        let f = std::fs::File::open(file_name).unwrap();
        let mut result: InstanceOfMapping = serde_yaml::from_reader(f).unwrap();
        for id in result.ignore.clone() {
            result.map.insert(id, "".to_string());
        }
        return result;
    }

    pub fn is_indexed(&self, id: &u64) -> bool {
        return self.do_not_index.contains(id);
    }

    pub fn get(&self, id: &u64) -> InstanceOfType {
        match self.map.get(&id) {
            Some(name) => {
                if name.eq("") {
                    return InstanceOfType::IgnoreEntity;
                }
                return InstanceOfType::Label(name);
            },
            None => InstanceOfType::NotFound,
        }
    }
}
