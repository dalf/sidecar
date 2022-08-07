extern crate indicatif;
extern crate serde;
extern crate serde_json;
extern crate simd_json;
// extern crate simd_json_derive;

// #[macro_use]
// extern crate serde_derive;

use std::collections::{HashSet};
use std::io::{self, BufRead, BufReader};
use std::{thread, time::Duration};

use documents::WDDocument;
use serde_json::Value;

// use simd_json_derive::Deserialize as SimdDeserialize;

const BATCH_SIZE: u64 = 2_000_000;

mod charabia_tokenizer;
mod default_values;
mod documents;
mod ranks;
mod string_to_id;
mod utils;
mod write;
mod instance_of;

#[cfg(not(windows))]
#[cfg(not(stage0))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn parse_item<'a>(ranks: &ranks::Ranks, instance_of_mapping: &instance_of::InstanceOfMapping, item: &'a documents::WikidataItem) -> Option<(String, WDDocument)> {
    let mut widtext = item.id.to_string();
    // TODO : store P(roperty) and L(exeme)
    // TODO : parse identifiers
    if widtext.chars().next().unwrap() != 'Q' {
        return None;
    }
    widtext.remove(0);
    let wid = widtext.parse::<u64>().unwrap();
    let mut add_to_index = true;
    let mut instance_of = Vec::new();

    for p31v in item.claims.p31.iter() {
        match p31v.mainsnak.datavalue.value.get("numeric-id") {
            None => {
                println!("{} has a broken P31 value", item.id);
            },
            Some(p31value) => {
                let p31u = p31value.as_u64().unwrap();
                match instance_of_mapping.get(&p31u) {
                    instance_of::InstanceOfType::IgnoreEntity => return None,
                    instance_of::InstanceOfType::NotFound => {},
                    instance_of::InstanceOfType::Label(_) => {},
                }
                if instance_of_mapping.is_indexed(&p31u) {
                    add_to_index = false;
                } else {
                    instance_of.push(p31u);
                }
            }
        }
    }

    let entity_rank = ranks.get(wid);
    let mut wddocument = documents::WDDocument::new(wid, entity_rank, instance_of, add_to_index);

    let mut seen_languages = HashSet::new();

    for (language, value) in item.labels.iter() {
        let label = value.value.to_string();
        wddocument.titles.push((language.to_string(), label));
        seen_languages.insert(language.to_string());
    }
    for (_language, list_of_values) in item.aliases.iter() {
        for value in list_of_values {
            seen_languages.insert(value.value.to_string());
            wddocument.titles.push(("".to_string(), value.value.to_string()));
        }
    }
    for (wikiname, value) in item.sitelinks.iter() {
        wddocument.sitelinks.push((wikiname.to_string(), value.title.to_string()));

        if !seen_languages.contains(&value.title) {
            seen_languages.insert(value.title.to_string());
            wddocument.titles.push(("".to_string(), value.title.to_string()));
        }
    }

    // Add the wddocument to the current batch
    let label_en = match item.labels.get("en") {
        Some(v) => v.value.to_string(),
        None => "".to_string(),
    };
    return Some((label_en, wddocument));
}

fn main() -> std::io::Result<()> {
    let mut reader = BufReader::with_capacity(8 * 1024 * 1024, io::stdin());
    let mut document_pool = documents::WDDocumentPool::new();
    let instance_of = instance_of::InstanceOfMapping::load_from("/data/wikidata/src/instance_of.yaml");
    let ranks = ranks::load("/data/wikidata/src/qrank.csv");
    let mut storage = write::TantivyStorage::open_or_create("/data/wikidata/db8".to_string());
    storage.set_instance_of(&instance_of);

    let bar = utils::new_progress_bar(100_000_000);
    let mut processing = true;

    // let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();

    while processing {
        let mut buffer: Vec<u8> = Vec::new();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(size) => {
                if size == 0 {
                    processing = false;
                    continue;
                }
                let json_buffer = &mut buffer[0..(size-2)];
                let json_result: Result<documents::WikidataItem, simd_json::Error> = simd_json::from_slice(json_buffer);
                match json_result {
                    Ok(json_document) => {
                        match parse_item(&ranks, &instance_of, &json_document) {
                            None => {},
                            Some((key, wddocument)) => {
                                document_pool.insert(key, &wddocument);
                            }
                        }
                        bar.inc(1);
                    }
                    Err(e) => {
                        // Error parsing JSON: try to get the id
                        let r2: Result<Value, simd_json::Error> = simd_json::from_slice(json_buffer);
                        match r2 {
                            Ok(doc) => {
                                match doc.get("id") {
                                    Some(id) => bar.println(format!("Error parsing JSON for entity {:?}: Error: {}", id, e)),
                                    None => bar.println(format!("Error parsing JSON: {}", e))
                                }
                            }
                            Err(_) => {
                                bar.println(format!("Error parsing JSON: {}\n{}\n", e, String::from_utf8(json_buffer.to_owned()).unwrap()))
                            }
                        }
                    }
                }
            },
            Err(e) => {
                bar.println(format!("Error:{}", e));
                processing = false;
            }
        }
        // update progress bar
        if document_pool.len % 10000 == 0 {
            bar.set_message(format!("{} %", ((100 * document_pool.len / BATCH_SIZE) as u64)));
        }

        //
        if document_pool.len >= BATCH_SIZE {
            // write buffer
            storage.write(&document_pool).unwrap();
            document_pool.clear();
        }
    }
    storage.write(&document_pool).unwrap();
    document_pool.clear();

    thread::sleep(Duration::from_secs(60));

    bar.finish();
    Ok(())
}
