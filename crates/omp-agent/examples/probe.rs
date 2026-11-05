//! Live protocol probe: spawn the sidecar, negotiate, then exercise the fork-only command surface.
//!
//! `cargo run -p omp-agent --example probe -- <path-to-omp.exe>`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use omp_agent::argv::{Capabilities, LaunchOptions, TabKind};
use omp_agent::sidecar::{Sidecar, SidecarOptions};
use omp_core::{Error, Result};
use omp_rpc::{RpcClient, supports_protocol_v2};
use parking_lot::Mutex;
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> Result<()> {
    let binary = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("resources/omp.exe"));
    let caps = Capabilities::fork();
    let launch = LaunchOptions {
        cwd: std::env::current_dir().map_err(Error::from)?,
        kind: TabKind::Agent,
        fresh_launch: true,
        ..LaunchOptions::default()
    };

    let ready: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
    let client_slot: Arc<Mutex<Option<Arc<RpcClient>>>> = Arc::new(Mutex::new(None));

    let on_frame: omp_agent::sidecar::FrameHandler = {
        let ready = ready.clone();
        let client_slot = client_slot.clone();
        Arc::new(move |frame: Value| {
            if frame.get("type").and_then(Value::as_str) == Some("ready") {
                *ready.lock() = Some(frame.clone());
            }
            if let Some(client) = client_slot.lock().as_ref() {
                client.on_response(&frame);
            }
        })
    };

    let sidecar = Sidecar::spawn(
        &SidecarOptions {
            binary,
            launch,
            caps: caps.clone(),
        },
        on_frame,
    )?;
    let client = Arc::new(RpcClient::new(sidecar.frame_sink()));
    *client_slot.lock() = Some(client.clone());

    for _ in 0..50 {
        if ready.lock().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    match ready.lock().clone() {
        Some(frame) => println!(
            "ready: protocolVersion={:?} v2={}",
            frame.get("protocolVersion").and_then(Value::as_u64),
            supports_protocol_v2(&frame)
        ),
        None => println!("ready: not observed within 5s (continuing)"),
    }

    let commands = [
        "get_state",
        "get_available_models",
        "get_settings_schema",
        "terminal_list",
        "get_jobs",
        "get_context_report",
        "browser_host_start",
    ];
    let mut failures = 0;
    for name in commands {
        let mut cmd = json!({ "type": name });
        let timeout = omp_rpc::timeout_for_command(name).or(Some(Duration::from_secs(20)));
        match client.command(&mut cmd, timeout).await {
            Ok(response) => {
                let ok = response.get("success").and_then(Value::as_bool).unwrap_or(false);
                if !ok {
                    failures += 1;
                }
                if name == "browser_host_start" {
                    println!("{name}: success={ok} data={}", response.get("data").cloned().unwrap_or_default());
                } else {
                    println!("{name}: success={ok}");
                }
            }
            Err(err) => {
                failures += 1;
                println!("{name}: error {err}");
            }
        }
    }

    sidecar.dispose().await;
    if failures > 0 {
        return Err(Error::msg(format!("{failures} probe command(s) failed")));
    }
    Ok(())
}
