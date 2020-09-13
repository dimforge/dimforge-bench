#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate bson;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate rocket;

use chrono::offset::TimeZone;
use chrono::{DateTime, Utc};
use dimforge_bench_common::{BenchCSVEntry, BenchConfig};
use log::LevelFilter;
use mongodb::sync::{Collection, Database};
use rocket::config::{Config, Environment};
use rocket::response::content::Json;
use rocket::State;
use simple_logger::SimpleLogger;

struct ServerState {
    db: Database,
}

fn main() {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    let config = BenchConfig::from_json_file(None);
    let db = connect_to_mongodb(&config.mongodb_server_uri, &config.mongodb_db).unwrap();
    let state = ServerState { db };

    let cors = rocket_cors::CorsOptions::default().to_cors().unwrap();
    let rocket_config = Config::build(Environment::Production)
        .address("127.0.0.1")
        .port(7878)
        .finalize()
        .unwrap();

    rocket::custom(rocket_config)
        .mount("/", routes![graph_csv, graph_list])
        .manage(state)
        .attach(cors)
        .launch();
}

fn query_latest_branch_date(
    collection: &Collection,
    branch: &str,
) -> mongodb::error::Result<Option<DateTime<Utc>>> {
    let agg_match = doc! {
        "$match": { "key.branch": branch }
    };
    let agg_group = doc! {
        "$group": {
            "_id": "$key.branch",
            "maxDate": { "$max": "$key.date" },
        }
    };

    if let Some(Ok(doc)) = collection
        .aggregate(vec![agg_match, agg_group], None)?
        .next()
    {
        Ok(doc.get_datetime("maxDate").ok().cloned())
    } else {
        Ok(None)
    }
}

#[get("/graph/csv?<project>&<date1>&<date2>&<otherEngines>")]
#[allow(non_snake_case)]
fn graph_csv(
    state: State<ServerState>,
    project: String,
    date1: i64,
    date2: i64,
    otherEngines: Option<bool>,
) -> Json<String> {
    #[derive(Clone, Serialize, Deserialize)]
    struct BenchCSVResult {
        entries1: Vec<BenchCSVEntry>,
        entries2: Vec<BenchCSVEntry>,
    }

    info!("Processing request: {}, {}, {}", project, date1, date2);
    let collection = state.db.collection(&project);
    let other_engines = otherEngines.unwrap_or(false);

    // 2. Retrieve all the documents at these dates for these branches.
    let mut filter1 = doc! {
        "key.date": Utc.timestamp_millis(date1)
    };
    let mut filter2 = doc! {
        "key.date": Utc.timestamp_millis(date2)
    };

    if !other_engines {
        filter1.insert("context.backend", "rapier");
        filter2.insert("context.backend", "rapier");
    }

    let docs1 = collection.find(filter1, None).unwrap();
    let docs2 = collection.find(filter2, None).unwrap();

    let entries1: Vec<_> = docs1
        .filter_map(|doc| doc.ok())
        .filter_map(|doc| bson::from_document::<BenchCSVEntry>(doc).ok())
        .collect();
    let entries2: Vec<_> = docs2
        .filter_map(|doc| doc.ok())
        .filter_map(|doc| bson::from_document::<BenchCSVEntry>(doc).ok())
        .collect();

    // 3. Build a JSON document with the corresponding infos and return it to the client.
    let result = BenchCSVResult { entries1, entries2 };

    Json(serde_json::to_string(&result).unwrap())
}

#[get("/list?<field>&<project>")]
fn graph_list(state: State<ServerState>, field: String, project: String) -> String {
    let db = &state.db;
    let collection = db.collection(&project);
    let values: Vec<_> = collection
        .distinct(&field, None, None)
        .unwrap()
        .into_iter()
        .map(|b| b.into_canonical_extjson())
        .collect();
    serde_json::to_string(&values).unwrap()
}

fn connect_to_mongodb(uri: &str, db: &str) -> mongodb::error::Result<Database> {
    use mongodb::sync::Client;
    let client = Client::with_uri_str(&uri)?;
    Ok(client.database(db))
}
