extern crate indicatif;
extern crate serde_derive;
extern crate serde_json;
extern crate tantivy;

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::iter::FromIterator;
use std::path::Path;

use serde_json::json;

use self::tantivy::directory::MmapDirectory;
use self::tantivy::schema::{IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FacetOptions, Facet, FAST, INDEXED, STORED};
use self::tantivy::store::Compressor;
use self::tantivy::{doc, Directory, Index, IndexBuilder, IndexSettings, IndexWriter, TantivyError};

use crate::documents::WDDocumentPool;
use crate::utils::new_progress_bar;
use crate::charabia_tokenizer;
use crate::default_values::{WIKIPEDIA_LANGUAGES, WIKIPEDIA_LANGUAGES_COUNT, WIKISITES};
use crate::string_to_id::StringToId;
use crate::instance_of::{InstanceOfMapping,InstanceOfType};


pub(crate) struct TantivyStorage {
    directory: String,
    index_entities: Index,
    index_labels: Index,
    entities_writer: IndexWriter,
    labels_writer: IndexWriter,
    stats: [usize; WIKIPEDIA_LANGUAGES_COUNT],
    stats_names: HashMap<String, usize>,
    sitelinks: StringToId,
    languages: StringToId,
    instance_of: InstanceOfMapping,
}

fn get_text_no_store(tokenizer_name: &str) -> TextOptions {
    return TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_fieldnorms(false)
            .set_tokenizer(tokenizer_name)
            .set_index_option(IndexRecordOption::Basic),
    );
}

/*
 Store a mapping from id to label.
 Except that multiple ids map to the same value.
 For example "en" -> "Douglas Adam", "fr" -> "Douglas Adam", "ja" -> "ダグラス・アダムズ" .
 is going to be map to 1 -> "Douglas Adam", 2 -> "Douglas Adam", 3 -> "ダグラス・アダムズ" using string_to_id

 Stored in the BTreeMap as
 * "Douglas Adam" -> [1, 2]
 * "ダグラス・アダムズ" ->  [3]

 Then serialized:
 * keys() : ["Douglas Adam", "ダグラス・アダムズ"]
 * values() : [[1, 2], 3]

 Notice that [3] is replaced by 3 to optimize then disk space.
*/
struct WeirdStruct {
    map: BTreeMap<String, Vec<u64>>,
}

impl WeirdStruct {
    fn new() -> WeirdStruct {
        WeirdStruct { map: BTreeMap::new() }
    }

    fn insert(&mut self, id: &str, label: &str, string_to_id: &mut StringToId) {
        let id = string_to_id.get(id.to_string());
        match self.map.get_mut(&label.to_string()) {
            Some(v) => {
                v.push(id);
            }
            None => {
                let mut v = Vec::new();
                v.push(id);
                self.map.insert(label.to_string(), v);
            }
        };
    }

    fn keys(&self) -> Vec<serde_json::Value> {
        Vec::from_iter(self.map.keys().map(|e| json!(e)))
    }

    fn values(&self) -> Vec<serde_json::Value> {
        Vec::from_iter(self.map.values().map(|e| {
            if e.len() == 1 {
                return json!(e.first());
            }
            let mut esort = e.clone();
            esort.sort();
            return json!(esort);
        }))
    }
}

impl TantivyStorage {
    pub fn open_or_create(directory: String) -> TantivyStorage {
        //
        let directory_path = Path::new(directory.as_str());

        let mut languages = StringToId::new();
        for language in WIKIPEDIA_LANGUAGES {
            languages.get(language.to_string());
        }

        let mut sitelinks = StringToId::new();
        for s in WIKISITES {
            sitelinks.get(s.to_string());
        }

        //
        let mut index = 0;
        let stats: [usize; WIKIPEDIA_LANGUAGES_COUNT] = [0; WIKIPEDIA_LANGUAGES_COUNT];
        let mut stats_names = HashMap::new();
        for language in WIKIPEDIA_LANGUAGES {
            stats_names.insert(language.to_string(), index);
            index += 1;
        }

        const CHARABIA_TOKENIZER: &str = "charabia";
        const CHARABIA_SEGMENT_TOKENIZER: &str = "charabia_segment";

        // entities
        let mut schema_builder = Schema::builder();
        schema_builder.add_u64_field("qid", INDEXED);
        schema_builder.add_json_field("body", get_text_no_store(&CHARABIA_SEGMENT_TOKENIZER));

        let schema_entities = schema_builder.build();
        let index_setting_entities: IndexSettings = IndexSettings {
            sort_by_field: None,
            docstore_compression: Compressor::Brotli,
            docstore_blocksize: 16384,
        };
        let mmap_entities: Box<dyn Directory> = Box::new(MmapDirectory::open(directory_path.join("entities")).unwrap());
        let index_entities = IndexBuilder::new()
            .schema(schema_entities)
            .settings(index_setting_entities)
            .open_or_create(mmap_entities)
            .unwrap();

        // labels
        schema_builder = Schema::builder();
        let title_options: TextOptions = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(&CHARABIA_TOKENIZER)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
        schema_builder.add_text_field("title", title_options);
        schema_builder.add_u64_field("qid", STORED);
        schema_builder.add_facet_field("instance_of", FacetOptions::default().set_stored());
        schema_builder.add_u64_field("rank", FAST);
        let schema_labels = schema_builder.build();
        let index_setting_alias: IndexSettings = IndexSettings {
            sort_by_field: None,
            docstore_compression: Compressor::Lz4,
            docstore_blocksize: 16384,
        };
        let mmap_labels: Box<dyn Directory> = Box::new(MmapDirectory::open(directory_path.join("labels")).unwrap());
        let index_labels = IndexBuilder::new()
            .schema(schema_labels)
            .settings(index_setting_alias)
            .open_or_create(mmap_labels)
            .unwrap();

        // register tokenizer : segment for entities, token for labels
        index_entities.tokenizers().register(
            &CHARABIA_SEGMENT_TOKENIZER,
            charabia_tokenizer::CharabiaSegmentTokenizer {},
        );
        index_labels
            .tokenizers()
            .register(&CHARABIA_TOKENIZER, charabia_tokenizer::CharabiaTokenizer {});

        //
        let entities_writer = index_entities.writer_with_num_threads(4, 1_000_000_000).unwrap();
        let labels_writer = index_labels.writer_with_num_threads(4, 400_000_000).unwrap();

        //
        return TantivyStorage {
            directory: directory,
            index_entities,
            index_labels,
            entities_writer,
            labels_writer,
            stats,
            stats_names,
            sitelinks,
            languages,
            instance_of: InstanceOfMapping::new()
        };
    }

    pub fn set_instance_of(&mut self, instance_of: &InstanceOfMapping) {
        self.instance_of = instance_of.clone();
    }

    pub fn write(&mut self, document_pool: &WDDocumentPool) -> Result<(), TantivyError> {
        let bar = new_progress_bar(document_pool.len);
        bar.set_message("Write index");

        let e_qid = self.index_entities.schema().get_field("qid").unwrap();
        let e_body = self.index_entities.schema().get_field("body").unwrap();

        let l_title = self.index_labels.schema().get_field("title").unwrap();
        let l_qid = self.index_labels.schema().get_field("qid").unwrap();
        let l_rank = self.index_labels.schema().get_field("rank").unwrap();
        let l_instance_of = self.index_labels.schema().get_field("instance_of").unwrap();

        for (_label, documents) in document_pool.documents.iter() {
            for doc in documents.iter() {
                let mut doc_labels: tantivy::Document = doc!(l_qid => doc.wid, l_rank => doc.rank);

                // labels & aliases
                let mut labels = WeirdStruct::new();
                for (language, title) in doc.titles.iter() {
                    if self.stats_names.contains_key(language) {
                        labels.insert(language, title, &mut self.languages);
                    }
                    doc_labels.add_text(l_title, title.to_string());
                }

                // Sitelinks
                let mut sitelinks = WeirdStruct::new();
                for (sitename, name) in doc.sitelinks.iter() {
                    sitelinks.insert(sitename, name, &mut self.sitelinks);
                }

                // Instance of
                for instance_of in doc.instance_of.iter() {
                    match self.instance_of.get(&instance_of.clone()) {
                        InstanceOfType::NotFound => {},
                        InstanceOfType::IgnoreEntity => {},
                        InstanceOfType::Label(io_str) => doc_labels.add_field_value(l_instance_of, Facet::from(io_str))
                    }
                }

                // add document to the writer
                let mut body = serde_json::Map::new();
                body.insert("a".to_string(), json!(labels.keys()));
                body.insert("b".to_string(), json!(sitelinks.keys()));
                body.insert("c".to_string(), json!(labels.values()));
                body.insert("d".to_string(), json!(sitelinks.values()));
                let mut doc_entity = doc!(e_qid => doc.wid);
                doc_entity.add_json_object(e_body, body);
                self.entities_writer.add_document(doc_entity)?;

                // ** Write the document for labels
                if doc.add_to_index {
                    self.labels_writer.add_document(doc_labels)?;
                }
            }
            bar.inc(1);
        }

        bar.set_message("commit");

        self.entities_writer.commit()?;
        self.labels_writer.commit()?;

        self.entities_writer.garbage_collect_files();
        self.labels_writer.garbage_collect_files();

        self.write_languages();
        self.write_sitelinks();
        self.write_stats();

        bar.finish_and_clear();
        Ok(())
    }

    fn write_stats(&self) {
        let mut writer = csv::Writer::from_path(Path::new(&self.directory).join("lang_stat.csv")).unwrap();
        for (language, index) in self.stats_names.iter() {
            let count = self.stats[*index];
            writer.serialize(&[count.to_string(), language.to_string()]).unwrap();
        }
        writer.flush().unwrap();
    }

    fn write_sitelinks(&self) {
        let writer = File::create(Path::new(&self.directory).join("sitelinks.json")).unwrap();
        ::serde_json::to_writer(&writer, &self.sitelinks).unwrap();
    }

    fn write_languages(&self) {
        let writer = File::create(Path::new(&self.directory).join("languages.json")).unwrap();
        ::serde_json::to_writer(&writer, &self.languages).unwrap();
    }
}
