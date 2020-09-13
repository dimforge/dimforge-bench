#[macro_use]
extern crate serde;

use bson::DateTime;
use std::fs::File;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchConfig {
    pub mongodb_bencher_uri: String,
    pub mongodb_server_uri: String,
    pub rabbitmq_uri: String,
    pub mongodb_db: String,
}

impl BenchConfig {
    pub fn from_json_file(path: Option<&str>) -> Self {
        let home = std::env::var("HOME").unwrap_or(String::new());
        let default_path = format!("{}/.dimforge/benchbot.json", home);
        let path = path.unwrap_or(&default_path);
        let file = File::open(path).expect("Could not open configuration file.");
        serde_json::from_reader(file).expect("Could not read configuration file as JSON.")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchMessage {
    pub repository: String,
    pub branch: String,
    pub commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchKey {
    /// The commit SHA where this benchmark is run.
    pub commit: String,
    /// The branch where this benchmark is run.
    pub branch: String,
    /// When this benchmark is run.
    pub date: DateTime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchContext {
    /// Name of what is being benched.
    pub name: String,
    /// The backend used for this benchmark.
    pub backend: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchPlatform {
    /// Compiler version used to run the benchmarks.
    pub compiler: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchCSVEntry {
    /// Benchmark key.
    pub key: BenchKey,
    /// Benchmark context.
    pub context: BenchContext,
    /// Details about the platforms the benchmark is run on.
    pub platform: BenchPlatform,
    /// Timings in milliseconds.
    pub timings: Vec<f32>,
    // TODO: also add the float type, simd, parallelism, and processor?
}
