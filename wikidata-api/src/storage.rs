#![feature(str_split_whitespace_as_str)]
extern crate tantivy;

use std::collections::HashMap;

use serde::Serialize;

use tantivy::collector::{Collector, FacetCollector, TopDocs};
use tantivy::fastfield::FastFieldReader;
use tantivy::query::FuzzyTermQuery;
use tantivy::schema::{Facet, Field};
use tantivy::{DocAddress, DocId, Document, Executor, LeasedItem, Score, Searcher, SegmentReader, Term};

use self::tantivy::query::QueryParser;
use self::tantivy::Index;

use super::charabia_tokenizer;
use super::string_to_id::StringToId;

#[derive(Serialize)]
pub struct DocumentEntity {
    labels: HashMap<String, String>,
    wikisites: HashMap<String, String>,
}

impl DocumentEntity {
    pub fn get_labels(&self) -> &HashMap<String, String> {
        &self.labels
    }

    pub fn get_wikisites(&self) -> &HashMap<String, String> {
        &self.wikisites
    }
}

pub(crate) struct TantivyStorage {
    index_entities: Index,
    index_labels: Index,
    searcher_entities: LeasedItem<Searcher>,
    searcher_labels: LeasedItem<Searcher>,
    query_parser_labels: QueryParser,
    query_parser_id: QueryParser,
    executor: Executor,
    label_to_id: StringToId,
    wikisite_to_id: StringToId,
}

impl TantivyStorage {
    pub fn new() -> TantivyStorage {
        // labels
        let index_entities = Index::open_in_dir("/data/wikidata/db_7/entities").unwrap();

        // aliases
        let index_labels = Index::open_in_dir("/data/wikidata/db_7/labels").unwrap();

        const CHARABIA_TOKENIZER: &str = "charabia";
        const CHARABIA_SEGMENT_TOKENIZER: &str = "charabia_segment";

        // register tokenizer : segment for entities, token for labels
        index_entities.tokenizers().register(
            &CHARABIA_SEGMENT_TOKENIZER,
            charabia_tokenizer::CharabiaSegmentTokenizer {},
        );
        index_labels
            .tokenizers()
            .register(&CHARABIA_TOKENIZER, charabia_tokenizer::CharabiaTokenizer {});

        //
        let reader_entities = index_entities.reader().unwrap();
        let reader_labels = index_labels.reader().unwrap();

        //
        let searcher_entities = reader_entities.searcher();
        let searcher_labels = reader_labels.searcher();

        //
        let mut fields2 = Vec::new();
        fields2.push(index_entities.schema().get_field("qid").unwrap());
        let query_parser_id = QueryParser::for_index(&index_entities, fields2);

        //
        let mut fields = Vec::new();
        fields.push(index_labels.schema().get_field("title").unwrap());
        let query_parser_labels = QueryParser::for_index(&index_labels, fields);

        //
        let executor = Executor::multi_thread(8, "tantity").unwrap();

        //
        return TantivyStorage {
            index_entities,
            index_labels,
            searcher_labels,
            searcher_entities,
            query_parser_labels,
            query_parser_id,
            executor,
            label_to_id: StringToId::load("/data/wikidata/db_7/languages.json".to_string()),
            wikisite_to_id: StringToId::load("/data/wikidata/db_7/sitelinks.json".to_string()),
        };
    }

    pub fn get_index_entities(&self) -> &Index {
        return &self.index_entities;
    }

    pub fn get_index_labels(&self) -> &Index {
        return &self.index_labels;
    }

    pub fn get_languages(&self) -> Vec<String> {
        self.label_to_id.keys()
    }

    fn collect_score_and_qid(&self, docs: Vec<(f32, DocAddress)>) -> Vec<(f32, u64, Facet)> {
        let mut result = Vec::new();
        let field_qid = self.index_labels.schema().get_field("qid").unwrap();
        let field_instance_of = self.index_labels.schema().get_field("instance_of").unwrap();
        for (score, doc_address) in docs {
            let retrieved_doc = self.searcher_labels.doc(doc_address).unwrap();
            let qid = retrieved_doc.get_first(field_qid).unwrap().as_u64().unwrap();
            let instance_of = match retrieved_doc.get_first(field_instance_of) {
                Some(f) => f.as_facet().unwrap().clone(),
                None => Facet::from_text("/").unwrap(),
            };
            result.push((score, qid, instance_of.clone()));
        }
        return result;
    }

    pub fn autocomplete(&self, query: String, distance: u8) -> Vec<(f32, u64, Facet)> {
        let field_title = self.index_labels.schema().get_field("title").unwrap();
        let term = Term::from_field_text(field_title, &query);
        let query = FuzzyTermQuery::new(term, distance, true);
        let docs = self
            .searcher_labels
            .search_with_executor(&query, &TopDocs::with_limit(10), &self.executor)
            .unwrap();
        return self.collect_score_and_qid(docs);
    }

    pub fn get_query_and_facet(&self, query: String) -> (String, Option<Facet>) {
        match query.split_once(" ") {
            None => return (query, None),
            Some((first_word, b)) => {
                if first_word.chars().nth(0).unwrap() != '/' {
                    return (query, None);
                }
                match Facet::from_text(first_word) {
                    Err(_) => return (query, None),
                    Ok(f) => return (b.to_string(), Some(f)),
                }
            }
        }
    }

    pub fn query(&self, query: String, use_rank: bool) -> Vec<(f32, u64, Facet)> {
        // let (q, f) = self.get_query_and_facet(query);
        // let field_instance_of = self.index_labels.schema().get_field("field_instance_of").unwrap();
        let query = self.query_parser_labels.parse_query(&query).unwrap();
        let field_rank: Field = self.index_labels.schema().get_field("rank").unwrap();
        let docs = match use_rank {
            false => {
                let top_collector = TopDocs::with_limit(20);
                self.searcher_labels.search_with_executor(&query, &top_collector, &self.executor)
            }
            true => {
                let collector = TopDocs::with_limit(20).tweak_score(move |segment_reader: &SegmentReader| {
                    let rank_reader = segment_reader.fast_fields().u64(field_rank).unwrap();
                    move |doc: DocId, original_score: Score| {
                        let rank: u64 = rank_reader.get(doc);
                        let rank_boost_score = ((2u64 + rank) as Score).log2().log2();
                        rank_boost_score * original_score
                    }
                });
                self.searcher_labels
                    .search_with_executor(&query, &collector, &self.executor)
            }
        };
        return self.collect_score_and_qid(docs.unwrap());
    }

    fn blob(
        &self,
        labels: &Vec<serde_json::Value>,
        ids: &Vec<serde_json::Value>,
        string_to_id: &StringToId,
    ) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for (label_value, lang_value) in labels.iter().zip(ids.iter()) {
            let label = label_value.as_str().unwrap();
            if lang_value.is_array() {
                for lang in lang_value.as_array().unwrap() {
                    let id = lang.as_u64().unwrap();
                    result.insert(string_to_id.get_string(id), label.to_string());
                }
            } else {
                let id = lang_value.as_u64().unwrap();
                result.insert(string_to_id.get_string(id), label.to_string());
            }
        }
        result
    }

    pub fn zz(&self, document: &Document, look_for_id: u64, label_key: &str, indice_key: &str) -> Option<String> {
        let field_body = self.index_entities.schema().get_field("body").unwrap();
        let body = document.get_first(field_body).unwrap().as_json().unwrap();
        let labels = body.get(label_key).unwrap();
        let indice_vec = body.get(indice_key).unwrap().as_array().unwrap();
        let mut found_indice: Option<usize> = None;
        'outer: for (i, indice) in indice_vec.iter().enumerate() {
            if indice.is_array() {
                for indice1 in indice.as_array().unwrap() {
                    let id = indice1.as_u64().unwrap();
                    if look_for_id == id {
                        found_indice = Some(i);
                        break 'outer;
                    }
                }
            } else {
                let id = indice.as_u64().unwrap();
                if look_for_id == id {
                    found_indice = Some(i);
                    break 'outer;
                }
            }
        }
        return match found_indice {
            Some(i) => Some(labels.get(i).unwrap().as_str().unwrap().to_string()),
            None => None,
        };
    }

    pub fn get_label_from_document(&self, document: &Document, language: &str) -> Option<String> {
        let id = self.label_to_id.get(language.to_string());
        return self.zz(document, id, "a", "c");
    }

    pub fn get_sitelink_from_document(&self, document: &Document, key: &str) -> Option<String> {
        let id = self.wikisite_to_id.get(key.to_string());
        return self.zz(document, id, "b", "d");
    }

    pub fn document_entity_to_map(&self, document: Document) -> DocumentEntity {
        let field_body = self.index_entities.schema().get_field("body").unwrap();
        let body = document.get_all(field_body).next().unwrap().as_json().unwrap();
        let labels = body.get("a").unwrap().as_array().unwrap();
        let labels_languages = body.get("c").unwrap().as_array().unwrap();
        let wikisites = body.get("b").unwrap().as_array().unwrap();
        let wikisites_ids = body.get("d").unwrap().as_array().unwrap();

        DocumentEntity {
            labels: self.blob(labels, labels_languages, &self.label_to_id),
            wikisites: self.blob(wikisites, wikisites_ids, &self.wikisite_to_id),
        }
    }

    pub fn entity_for(&self, id: String) -> Option<Document> {
        let query = self.query_parser_id.parse_query(&id).unwrap();
        let docs = self
            .searcher_entities
            .search_with_executor(&query, &TopDocs::with_limit(1), &self.executor)
            .unwrap();
        return match docs.get(0) {
            Some((_score, doc_address)) => Some(self.searcher_entities.doc(*doc_address).unwrap()),
            None => None,
        };
    }
}
