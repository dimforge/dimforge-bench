#[macro_use]
extern crate log;

use amiquip::{
    Connection, ConsumerMessage, ConsumerOptions, Exchange, Publish, QueueDeclareOptions,
};
use bson::DateTime;
use clap::{App, Arg, SubCommand};
use dimforge_bench_common::{
    BenchCSVEntry, BenchConfig, BenchContext, BenchKey, BenchMessage, BenchPlatform,
};
use log::LevelFilter;
use mongodb::sync::Database;
use simple_logger::SimpleLogger;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

fn main() -> mongodb::error::Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    let matches = App::new("Dimforge benchmark tool")
        .arg(
            Arg::with_name("config")
                .short("f")
                .required(false)
                .takes_value(true)
                .help("path to the JSON configuration file"),
        )
        .subcommand(SubCommand::with_name("configure").about("Configure credentials"))
        .subcommand(
            SubCommand::with_name("send")
                .about("Send a message to start a benchmark")
                .arg(
                    Arg::with_name("repository")
                        .short("r")
                        .required(true)
                        .takes_value(true)
                        .help("the repository to clone"),
                )
                .arg(
                    Arg::with_name("branch")
                        .short("b")
                        .required(true)
                        .takes_value(true)
                        .help("the branch of the commit to compile"),
                )
                .arg(
                    Arg::with_name("commit")
                        .short("c")
                        .required(true)
                        .takes_value(true)
                        .help("the commit to compile"),
                ),
        )
        .subcommand(SubCommand::with_name("listen").about("Listen to incoming benchmark messages"))
        .get_matches();

    let config = matches.value_of("config");
    let config = BenchConfig::from_json_file(config);

    if let Some(matches) = matches.subcommand_matches("send") {
        let repository = matches.value_of("repository").unwrap().to_string();
        let branch = matches.value_of("branch").unwrap().to_string();
        let commit = matches.value_of("commit").unwrap().to_string();
        let message = BenchMessage {
            repository,
            branch,
            commit,
        };
        send_bench_message(&config, &message);
        info!("Bench message sent.");
    }

    if let Some(_) = matches.subcommand_matches("listen") {
        listen_bench_messages(&config)?;
    }

    if let Some(_) = matches.subcommand_matches("configure") {
        configure();
    }

    Ok(())
}

fn configure() {
    println!("MongoDB bencher uri: ");
    let mongodb_bencher_uri = text_io::read!("{}\n");
    println!("MongoDB server uri: ");
    let mongodb_server_uri = text_io::read!("{}\n");
    println!("MongoDB database: ");
    let mongodb_db = text_io::read!("{}\n");
    println!("Rabbitmq uri: ");
    let rabbitmq_uri = text_io::read!("{}\n");
    println!("Save configuration to folder [$HOME/.dimforge]: ");
    let mut output_dir: String = text_io::read!("{}\n");
    if output_dir.is_empty() {
        let home = std::env::var("HOME").unwrap_or(String::new());
        output_dir = format!("{}/.dimforge", home);
    }

    let config = BenchConfig {
        mongodb_db,
        mongodb_bencher_uri,
        mongodb_server_uri,
        rabbitmq_uri,
    };

    let config_json = serde_json::to_string(&config).unwrap();
    std::fs::create_dir_all(&output_dir).unwrap();
    let output_file = format!("{}/benchbot.json", output_dir);
    let mut out = File::create(&output_file).expect(
        "Could not open target configuration file. Did you run the `configure` subcommand yet?",
    );
    out.write_all(config_json.as_bytes()).unwrap();
    info!("Configuration successfully saved to '{}'.", output_file);
}

fn send_bench_message(config: &BenchConfig, message: &BenchMessage) {
    let mut connection = Connection::open(&config.rabbitmq_uri).unwrap();
    let channel = connection.open_channel(None).unwrap();
    let exchange = Exchange::direct(&channel);
    let message = serde_json::to_string(message).unwrap();
    exchange
        .publish(Publish::new(message.as_bytes(), "benchmark"))
        .unwrap();
    let _ = connection.close();
}

fn listen_bench_messages(config: &BenchConfig) -> mongodb::error::Result<()> {
    let mut connection = Connection::open(&config.rabbitmq_uri).unwrap();
    let channel = connection.open_channel(None).unwrap();
    let queue = channel
        .queue_declare("benchmark", QueueDeclareOptions::default())
        .unwrap();
    let consumer = queue.consume(ConsumerOptions::default()).unwrap();

    for message in consumer.receiver().iter() {
        match message {
            ConsumerMessage::Delivery(delivery) => {
                let body = String::from_utf8_lossy(&delivery.body);
                let message = serde_json::from_str::<BenchMessage>(&body);

                if delivery.redelivered {
                    // FIXME: add a retry count.
                    warn!("Dropping redelivered message: {:?}", message);
                    let _ = delivery.ack(&channel);
                    continue;
                }

                let message = message.unwrap();

                info!("Received bench message: {:?}", message);
                let tempdir = tempfile::tempdir().unwrap();
                let target_dir = tempdir.path();
                let bench_subdir = "benchmarks3d";

                let bench_names = clone_and_build_benches(
                    target_dir,
                    bench_subdir,
                    &message.repository,
                    &message.commit,
                );
                info!("About to run benchmarks: {:?}", bench_names);

                let version = rustc_version::version()
                    .map(|v| format!("{}", v))
                    .unwrap_or("unknown".to_string());

                let platform = BenchPlatform {
                    compiler: version.clone(),
                };

                let key = BenchKey {
                    commit: message.commit,
                    branch: message.branch,
                    date: DateTime(chrono::Utc::now()),
                };

                for bench_name in bench_names {
                    let context = BenchContext {
                        name: bench_name,
                        backend: String::new(), // Will be set later.
                    };
                    run_bench(config, target_dir, bench_subdir, &key, &context, &platform)?;
                }

                delivery.ack(&channel).unwrap();
            }
            other => {
                error!("consumer ended: {:?}", other);
                break;
            }
        }
    }

    let _ = connection.close();

    Ok(())
}

// Returns the name of all the benchmarks we can run.
fn clone_and_build_benches(
    repo_dir: &Path,
    bench_subdir: &str,
    repo_url: &str,
    commit: &str,
) -> Vec<String> {
    info!("Cloning {} in {:?}", repo_url, repo_dir);
    Command::new("git")
        .arg("clone")
        .arg(repo_url)
        .arg(repo_dir)
        .status()
        .unwrap();
    Command::new("git")
        .arg("checkout")
        .arg(commit)
        .current_dir(repo_dir)
        .status()
        .unwrap();

    let build_path = format!("{}/{}", repo_dir.to_string_lossy(), bench_subdir);
    info!("Building {}", build_path);
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .args(&["--features", "simd-nightly"])
        .args(&["--features", "other-backends"])
        .current_dir(&build_path)
        .status()
        .unwrap();
    info!("Build ended with status: {}", status);

    let exec_path = format!("{}/target/release", repo_dir.to_string_lossy());
    let output = Command::new("./all_benchmarks3")
        .arg("--list")
        .current_dir(exec_path)
        .output()
        .unwrap();

    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

fn run_bench(
    config: &BenchConfig,
    bench_dir: &Path,
    bench_subdir: &str,
    key: &BenchKey,
    context: &BenchContext,
    platform: &BenchPlatform,
) -> mongodb::error::Result<()> {
    let build_path = format!("{}/{}", bench_dir.to_string_lossy(), bench_subdir);

    let status = Command::new("cargo")
        .arg("run")
        .arg("--release")
        .args(&["--features", "simd-nightly"])
        .args(&["--features", "other-backends"])
        .args(&["--", "--bench", "--example", &context.name])
        .current_dir(build_path)
        .status()
        .unwrap();
    info!("Exit status for '{}' benchmark: {}", context.name, status);

    let entries = parse_results(bench_dir, bench_subdir, key, context, platform);
    upload_results(&config, &entries)
}

fn parse_results(
    repo_dir: &Path,
    bench_subdir: &str,
    key: &BenchKey,
    context: &BenchContext,
    platform: &BenchPlatform,
) -> Vec<BenchCSVEntry> {
    let bench_result_path = format!(
        "{}/{}/{}.csv",
        repo_dir.to_string_lossy(),
        bench_subdir,
        context.name
    );
    info!("Parting bench file: {}", bench_result_path);
    let csv = parse_csv(bench_result_path).unwrap();

    let mut entries = Vec::new();
    for (backend, timings) in csv.0.into_iter().zip(csv.1.into_iter()) {
        let mut context = context.clone();
        context.backend = backend;
        let entry = BenchCSVEntry {
            key: key.clone(),
            context,
            platform: platform.clone(),
            timings,
        };

        entries.push(entry);
    }

    entries
}

fn upload_results(config: &BenchConfig, entries: &[BenchCSVEntry]) -> mongodb::error::Result<()> {
    let db = connect_to_mongodb(&config.mongodb_bencher_uri, &config.mongodb_db)?;
    let coll = db.collection("rapier3d");

    for entry in entries {
        let doc = bson::to_document(entry).unwrap();
        coll.insert_one(doc, None)?;
    }

    Ok(())
}

fn parse_csv(path: impl AsRef<Path>) -> csv::Result<(Vec<String>, Vec<Vec<f32>>)> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)
        .unwrap();
    let headers: Vec<_> = reader.headers()?.iter().map(|h| h.to_string()).collect();
    let mut values = vec![Vec::new(); headers.len()];

    for record in reader.records() {
        for (i, value) in record?.iter().enumerate() {
            let val = f32::from_str(value).unwrap();
            values[i].push(val);
        }
    }

    Ok((headers, values))
}

fn connect_to_mongodb(uri: &str, db: &str) -> mongodb::error::Result<Database> {
    use mongodb::sync::Client;
    let client = Client::with_uri_str(uri)?;
    Ok(client.database(db))
}
