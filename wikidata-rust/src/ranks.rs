use std::{collections::HashMap};

use crate::utils::new_progress_bar;

pub struct Ranks {
    pub ranks: HashMap<u64, u64>,
}

impl Ranks {
    pub fn new() -> Ranks {
        Ranks {
            ranks: HashMap::new(),
        }
    }

    #[inline]
    pub fn get(&self, wid: u64) -> u64 {
        match self.ranks.get(&wid) {
            Some(rank) => *rank,
            None => 0,
        }
    }
}

pub fn load(file_name: &str) -> Ranks {
    // Build the CSV reader and iterate over each record.
    let mut rdr = csv::Reader::from_path(file_name).unwrap();
    let mut ranks = Ranks::new();
    let bar = new_progress_bar(26_000_000);
    bar.set_message("Loading ranks");
    for result in rdr.records() {
        // The iterator yields Result<StringRecord, Error>, so we check the
        // error here.
        let record = result.unwrap();
        let mut id = record.get(0).unwrap().to_string();
        id.remove(0);
        let nid = id.parse::<u64>().unwrap();
        let rank = record.get(1).unwrap().parse::<u64>().unwrap();
        ranks.ranks.insert(nid, rank);
        bar.inc(1);
    }
    bar.finish_and_clear();
    return ranks;
}
