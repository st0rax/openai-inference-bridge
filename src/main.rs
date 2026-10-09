use std::process;

use openai_inference_bridge::{config::Config, http_server};

fn main() {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("configuration failed: {error}");
            process::exit(2);
        }
    };

    if let Err(error) = http_server::run(config.bind_addr) {
        eprintln!("HTTP server failed: {error}");
        process::exit(1);
    }
}
