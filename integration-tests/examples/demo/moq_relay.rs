use std::time::Duration;

use anyhow::Result;
use inquire::Confirm;
use reqwest::Url;
use tracing::{info, warn};

pub const MOQ_RELAY_DEFAULT_URL: &str = "https://localhost:4443";

/// Blocks until the relay at `endpoint_url` responds, printing how to start a local one. Returns
/// the fingerprint of the relay's self-signed certificate, if it serves one.
pub fn wait_for_relay(endpoint_url: &str) -> Result<Option<String>> {
    let url = Url::parse(endpoint_url)?;
    let host = url.host_str().unwrap_or("localhost");
    let port = url.port_or_known_default().unwrap_or(443);
    let fingerprint_url = format!("http://{host}:{port}/certificate.sha256");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()?;

    loop {
        let response = client
            .get(&fingerprint_url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text());
        if let Ok(fingerprint) = response {
            info!("MoQ relay is running at {endpoint_url}.");
            warn!(
                "Relay uses a self-signed certificate, Smelter needs to run with \
                SMELTER_MOQ_DISABLE_TLS_VERIFICATION=true to connect to it."
            );
            return Ok(Some(fingerprint.trim().to_string()));
        }

        let args = format!(
            "--server-bind '[::]:{port}' --tls-generate {host} --web-http-listen '[::]:{port}' --auth-public anon"
        );
        println!("No MoQ relay found at {endpoint_url}. Start one in a separate terminal:");
        println!();
        println!("  # inside nix dev shell, or after `cargo install moq-relay`");
        println!("  moq-relay {args}");
        println!();
        println!("  # from the repository root, without the dev shell");
        println!("  nix run ./tools/nix#moq-relay -- {args}");
        println!();

        let retry = Confirm::new("Check again? (n to continue without a local relay) [Y/n]")
            .with_default(true)
            .prompt()?;
        if !retry {
            return Ok(None);
        }
    }
}

/// URL of a smelter-labs tools page prefilled with relay connection details.
pub fn tools_url(
    tool: &str,
    endpoint_url: &str,
    broadcast_path: &str,
    fingerprint: Option<&str>,
) -> String {
    let url = format!(
        "https://smelter-labs.github.io/tools/#{tool}?url={endpoint_url}&path={broadcast_path}"
    );
    match fingerprint {
        Some(fingerprint) => format!("{url}&cert={fingerprint}"),
        None => url,
    }
}
