/// # Wikidata local database
/// This is the API for a local copy of Wikidata.
/// 
/// Data are stored using Tantivy in two different indexes
/// * One contains the full labels and sitelinks. It is a compacted way.
/// 
/// * Another contains the indexes to retreive the Wikidata Ids.
#[macro_use]
extern crate rocket;
extern crate reqwest;
extern crate serde;
extern crate wikidata;

use rocket::serde::json::Json;
use rocket::{futures::future};
use rocket::response::content;
use rocket::State;

use rocket_okapi::rapidoc::{make_rapidoc, RapiDocConfig, GeneralConfig, HideShowConfig};
use rocket_okapi::settings::UrlObject;
use rocket_okapi::swagger_ui::{make_swagger_ui, SwaggerUIConfig};
use rocket_okapi::{openapi_get_routes, openapi};
use rocket_okapi::okapi::schemars;
use rocket_okapi::okapi::schemars::JsonSchema;
use rocket_dyn_templates::{Template, context};
use serde::{Serialize, Deserialize};
use serde_json::{self, Value};
use tantivy::Document;

mod charabia_tokenizer;
mod storage;
mod string_to_id;

struct Config {
    storage: storage::TantivyStorage,
    http_client: reqwest::Client,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct QueryResponseEntity {
    qid: u64,
    score: f32,
    title: String,
    instance_of: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct QueryResponse {
    query: String,
    entities: Vec<QueryResponseEntity>,
}

#[get("/")]
fn index() -> Template {
    let context = context! { name: "test", title: "title"};
    Template::render("index", &context)
}

#[get("/infobox/<id>")]
async fn infobox_html(config: &State<Config>, id: &str) -> Template {
    match config.storage.entity_for(id.to_string()) {
        Some(document) => {
            let infobox = get_infobox(config, &document, "en", id).await;
            let title = infobox.title.to_string();
            let context = context! { infobox: infobox, title: title};
            Template::render("infobox", &context)
        },
        None => {
            let context = context! { title: "Not found"};
            Template::render("notfound", &context)
        },
    }
    
}

/// # Return the list of supported languages
/// The local database has indexed all the wikidata languages
/// However it can retreive only some languages (for now)
/// This API returns this list.
/// 
/// The language parameter in the other endpoints must be only of the value of this list.
#[openapi(tag = "Meta")]
#[get("/languages")]
fn language_list(config: &State<Config>) -> Json<Vec<String>> {
    Json(config.storage.get_languages())
}

#[openapi(tag = "Local")]
#[get("/ac/<query>")]
fn autocomplete(config: &State<Config>, query: String) -> content::RawJson<String> {
    let mut entities = Vec::new();
    let query_result = query.clone();
    let field_title_en = config
        .storage
        .get_index_entities()
        .schema()
        .get_field("title_en")
        .unwrap();
    let mut distance = 1;
    let mut docs = Vec::new();
    while docs.len() == 0 && distance < 3 {
        docs = config.storage.autocomplete(query_result.to_string(), distance);
        distance += 1;
    }
    for (score, qid, instance_of) in docs {
        let document_label = config.storage.entity_for(qid.to_string()).unwrap();
        let title = match document_label.get_first(field_title_en) {
            Some(a) => a.as_text().unwrap().to_string(),
            None => "??".to_string(),
        };
        entities.push(QueryResponseEntity { qid, score, title, instance_of: instance_of.to_path_string() });
    }
    let response = &QueryResponse { query, entities };
    let response_json = serde_json::to_string(&response).unwrap().to_string();
    return content::RawJson(response_json);
}


fn resolve_query(config: &State<Config>, rank: bool, query: &str, language: &str) -> QueryResponse {
    let docs = config.storage.query(query.to_string(), rank);
    let mut entities = Vec::with_capacity(docs.len());
    for (score, qid, instance_of) in docs {
        let document_label = config.storage.entity_for(qid.to_string()).unwrap();
        let title = match config.storage.get_label_from_document(&document_label, language) {
            Some(a) => a,
            None => "".to_string(),
        };
        entities.push(QueryResponseEntity {
            qid,
            score,
            title: title.to_string(),
            instance_of: instance_of.to_path_string(),
        });
    }
    return QueryResponse { query: query.to_string(), entities };
}

/// # Search in the local data
/// * when pop is true, the entities are ranked using the score provided by https://qrank.wmcloud.org/
/// 
/// * language is "en" by default. It changed only the returned labels, it does not changed the search results.
/// 
/// * q : for the query syntax see https://docs.rs/tantivy/latest/tantivy/query/struct.QueryParser.html
#[openapi(tag = "Local")]
#[get("/search?<q>&<pop>&<language>")]
fn query(config: &State<Config>, pop: Option<bool>, q: &str, language: Option<&str>) -> Json<QueryResponse> {
    let actual_language = language.or(Some("en")).unwrap();
    let actual_pop = pop.or(Some(false)).unwrap();
    Json(resolve_query(config, actual_pop, q, actual_language))
}

/// # Fetch local data about an Wikidata entity
#[openapi(tag = "Local")]
#[get("/entity/<id>")]
fn entity(config: &State<Config>, id: String) -> content::RawJson<String> {
    let response_json = match config.storage.entity_for(id) {
        Some(document) => {
            let md = config.storage.document_entity_to_map(document);
            serde_json::to_string(&md).unwrap().to_string()
        }
        None => "{}".to_string(),
    };
    return content::RawJson(response_json);
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct InfoBox {
    title: String,
    thumbnail: String,
    content: String,
    attributes: Vec<InfoBoxAttribute>,
    urls: Vec<InfoBoxUrl>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct InfoBoxAttribute {
    property: String,
    values: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct InfoBoxUrl {
    name: String,
    value: String,
}

impl InfoBox {
    fn new(title: &str, thumbnail: &str, content: &str) -> InfoBox {
        InfoBox {
            title: title.to_string(),
            thumbnail: thumbnail.to_string(),
            content: content.to_string(),
            attributes: Vec::new(),
            urls: Vec::new(),
        }
    }

    fn add_attribute(&mut self, name: &str, values: &Vec<String>) {
        if values.len() > 0 {
            self.attributes.push(InfoBoxAttribute {
                property: name.to_string(),
                values: values.to_vec(),
            });
        }
    }

    fn add_url(&mut self, name: &str, value: &str) {
        self.urls.push(InfoBoxUrl {
            name: name.to_string(),
            value: value.to_string(),
        });
    }
}

fn get_property_values(document: &Value, property: &str) -> Vec<u64> {
    match document.get("claims").unwrap().get(property) {
        None => Vec::new(),
        Some(values) => {
            let mut normal = Vec::new();
            let mut preferred = Vec::new();

            for v in values.as_array().unwrap() {
                let rank = match v.get("rank") {
                    None => "normal",
                    Some(rank) => rank.as_str().unwrap(),
                };
                let a = v
                    .get("mainsnak")
                    .unwrap()
                    .get("datavalue")
                    .unwrap()
                    .get("value")
                    .unwrap();
                match a.get("numeric-id") {
                    None => {}
                    Some(numeric_id) => match rank {
                        "normal" => normal.push(numeric_id.as_u64().unwrap()),
                        "preferred" => preferred.push(numeric_id.as_u64().unwrap()),
                        _ => {}
                    },
                }
            }
            if preferred.len() > 0 {
                preferred
            } else {
                normal
            }
        }
    }
}

fn resolve_id(config: &State<Config>, lang: &str, qid: &u64) -> String {
    match config.storage.entity_for(qid.to_string()) {
        None => qid.to_string(),
        Some(document) => match config.storage.get_label_from_document(&document, lang) {
            None => qid.to_string(),
            Some(a) => a,
        },
    }
}

async fn send_multiple_urls(client: &reqwest::Client, urls: Vec<String>) -> Vec<Value> {
    future::join_all(urls.into_iter().map(|url| {
        let client = &client;
        async move {
            let resp = client.get(url).send().await.unwrap();
            let value: Value = resp.json().await.unwrap();
            value
        }
    }))
    .await
}

/*
 use parallel request:
 https://stackoverflow.com/questions/51044467/how-can-i-perform-parallel-asynchronous-http-get-requests-with-reqwest
*/
async fn get_infobox(config: &State<Config>, document: &Document, lang: &str, id: &str) -> InfoBox {
    let mut infobox = InfoBox::new("", "", "");

    let mut wikipedia_id = lang.to_string();
    wikipedia_id.push_str("wiki");

    let mut urls: Vec<String> = Vec::new();
    urls.push(format!("https://www.wikidata.org/wiki/Special:EntityData/Q{}.json", id));

    infobox.title = config
        .storage
        .get_label_from_document(&document, &lang)
        .or(Some("".to_string()))
        .unwrap();
    match config.storage.get_sitelink_from_document(&document, &wikipedia_id) {
        Some(wikipedia_page) => {
            urls.push(format!(
                "https://{}.wikipedia.org/api/rest_v1/page/summary/{}",
                lang, wikipedia_page
            ));
            infobox.add_url(
                "wikipedia",
                &format!("https://{}.wikipedia.org/wiki/{}", lang, wikipedia_page),
            );
        }
        None => {}
    }

    let responses = send_multiple_urls(&config.http_client, urls).await;
    let wikidata_response = responses.get(0);
    let wikpedia_response = responses.get(1);

    let wikidata_doc_meta = wikidata_response.unwrap().get("entities").unwrap().as_object().unwrap();
    let wikidata_entry = wikidata_doc_meta.iter().next().unwrap();
    let wikidata_doc = wikidata_entry.1;
    // use the QID from the REST API response because there are some redirect (for example : Q4755847 -> Q4255688)
    let wikidata_id = wikidata_entry.0;
    infobox.add_url("wikidata", format!("https://www.wikidata.org/wiki/{}", wikidata_id).as_str());
    
    match wikpedia_response {
        None => {},
        Some(resp) => {
            match resp.get("extract") {
                None => {}
                Some(extract) => {
                    infobox.content = extract.as_str().unwrap().to_string();
                }
            }
            match resp.get("thumbnail") {
                None => {}
                Some(thumbnail_value) => {
                    infobox.thumbnail = thumbnail_value.get("source").unwrap().as_str().unwrap().to_string();
                }
            }
        }
    }

    const ATTRIBUTE_LIST: [&str; 30] = [
        "P27", "P495", "P31", "P17", "P6", "P122", "P1454", "P400", "P50", "P170", "P57", "P175", "P178", "P162",
        "P176", "P58", "P272", "P264", "P123", "P449", "P750", "P86", "P38", "P136", "P364", "P277", "P840", "P282",
        "P218", "P225",
    ];
    for attribute_name in ATTRIBUTE_LIST {
        let values: Vec<String> = get_property_values(wikidata_doc, attribute_name)
            .iter()
            .map(|qid| resolve_id(config, lang, qid))
            .collect();
        infobox.add_attribute(attribute_name, &values);
    }
    return infobox;
}


/// # Display the infobox of the first result
/// Combine /search and /infobox
#[openapi(tag = "Local & External")]
#[get("/searchinfobox?<q>&<language>&<pop>")]
async fn searchinfo(config: &State<Config>, language: Option<&str>, q: &str, pop: Option<bool>) -> Option<Json<InfoBox>> {
    let actual_language = language.or(Some("en")).unwrap();
    let actual_pop = pop.or(Some(false)).unwrap();
    let response = resolve_query(config, actual_pop, q, actual_language);
    match response.entities.iter().next() {
        None => None,
        Some(e) => {
            let document = config.storage.entity_for(e.qid.to_string()).unwrap();
            let infobox = get_infobox(config, &document, actual_language, e.qid.to_string().as_str()).await;
            Some(Json(infobox))
        }
    }
}

/// # Display infobox
/// sing the local database, this endpoint makes two HTTP requests: one to the wikidata REST API, one to the wikipedia REST API.
/// The results are merged. The referenced entity ids are converted to string using the local database.
#[openapi(tag = "Local & External")]
#[get("/infobox/<lang>/<id>")]
async fn info(config: &State<Config>, lang: &str, id: &str) -> Option<Json<InfoBox>> {
    match config.storage.entity_for(id.to_string()) {
        Some(document) => Some(Json(get_infobox(config, &document, lang, id).await)),
        None => None,
    }
}

#[launch]
fn rocket() -> _ {
    let state = Config {
        http_client: reqwest::Client::builder().http2_prior_knowledge().build().unwrap(),
        storage: storage::TantivyStorage::new(),
    };
    rocket::build()
        .manage(state)
        .mount("/", routes![index, infobox_html])
        .mount("/api", openapi_get_routes![language_list, autocomplete, query, entity, info, searchinfo])
        .mount(
            "/swagger-ui/",
            make_swagger_ui(&SwaggerUIConfig {
                url: "../api/openapi.json".to_owned(),
                ..Default::default()
            }),
        )
        .mount(
            "/rapidoc/",
            make_rapidoc(&RapiDocConfig {
                general: GeneralConfig {
                    spec_urls: vec![UrlObject::new("General", "../api/openapi.json")],
                    ..Default::default()
                },
                hide_show: HideShowConfig {
                    allow_spec_url_load: false,
                    allow_spec_file_load: false,
                    ..Default::default()
                },
                ..Default::default()
            }),
        )
        .attach(Template::fairing())
}
