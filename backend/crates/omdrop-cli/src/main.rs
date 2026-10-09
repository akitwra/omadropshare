use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use omdrop_core::{IpcRequest, IpcResponse, StatusSnapshot, IPC_VERSION};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Debug, Parser)]
#[command(
    name = "omdropctl",
    version,
    about = "Control and diagnose OmarchyDrop"
)]
struct Cli {
    #[arg(long, global = true)]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Status(JsonFlag),
    Events {
        #[arg(long)]
        jsonl: bool,
    },
    Peers(JsonFlag),
    Adapters(JsonFlag),
    Discover {
        #[arg(long, default_value_t = 15)]
        timeout: u64,
    },
    Send {
        #[arg(long)]
        peer: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    Receive {
        #[command(subcommand)]
        command: ReceiveCommand,
    },
    Hardware {
        #[command(subcommand)]
        command: HardwareCommand,
    },
    Radio {
        #[command(subcommand)]
        command: RadioCommand,
    },
    Diagnostics(JsonFlag),
    Recover,
}

#[derive(Debug, Args)]
struct JsonFlag {
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum ReceiveCommand {
    On {
        #[arg(long, default_value_t = 600)]
        seconds: u64,
    },
    Off,
}

#[derive(Debug, Subcommand)]
enum HardwareCommand {
    Probe(JsonFlag),
    Test {
        #[arg(long)]
        adapter: Option<String>,
        #[arg(
            long,
            help = "Reserved for a future disruptive injection test; currently returns ACTIVE_TEST_UNAVAILABLE"
        )]
        active: bool,
    },
}

#[derive(Debug, Subcommand)]
enum RadioCommand {
    Start {
        #[arg(long)]
        adapter: String,
    },
    Stop {
        #[arg(long)]
        adapter: String,
    },
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let socket = cli
        .socket
        .unwrap_or_else(omdrop_platform_linux::default_socket_path);

    match cli.command {
        Command::Events { .. } => stream_events(&socket).await,
        Command::Status(flag) => {
            let value = call(&socket, "status", Value::Null).await?;
            print_status(value, flag.json)
        }
        Command::Peers(flag) => print_value(call(&socket, "peers", Value::Null).await?, flag.json),
        Command::Adapters(flag) => {
            let value = call(&socket, "adapters", Value::Null).await?;
            if flag.json {
                print_json(&value)
            } else {
                let snapshot: StatusSnapshot = serde_json::from_value(value)?;
                if snapshot.adapters.is_empty() {
                    println!("No Wi-Fi adapters detected");
                }
                for adapter in snapshot.adapters {
                    println!(
                        "{}\t{}\t{:?}\t{:?}",
                        adapter.interface.as_deref().unwrap_or("-"),
                        adapter.driver.as_deref().unwrap_or("unknown"),
                        adapter.bus,
                        adapter.level
                    );
                    for reason in adapter.reasons {
                        println!("  {reason}");
                    }
                }
                Ok(())
            }
        }
        Command::Discover { timeout } => {
            print_json(&call(&socket, "discover", json!({"timeout": timeout})).await?)
        }
        Command::Send { peer, files } => {
            let files = files
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            print_json(&call(&socket, "send", json!({"peer": peer, "files": files})).await?)
        }
        Command::Receive { command } => match command {
            ReceiveCommand::On { seconds } => {
                print_json(&call(&socket, "receive_on", json!({"seconds": seconds})).await?)
            }
            ReceiveCommand::Off => print_json(&call(&socket, "receive_off", Value::Null).await?),
        },
        Command::Hardware { command } => match command {
            HardwareCommand::Probe(flag) => {
                let value = call(&socket, "hardware_probe", Value::Null).await?;
                if flag.json {
                    print_json(&value)
                } else {
                    print_status(value, false)
                }
            }
            HardwareCommand::Test { adapter, active } => print_json(
                &call(
                    &socket,
                    "hardware_test",
                    json!({"adapter": adapter, "active": active}),
                )
                .await?,
            ),
        },
        Command::Radio { command } => match command {
            RadioCommand::Start { adapter } => {
                print_json(&call(&socket, "radio_start", json!({"adapter": adapter})).await?)
            }
            RadioCommand::Stop { adapter } => {
                print_json(&call(&socket, "radio_stop", json!({"adapter": adapter})).await?)
            }
            RadioCommand::Status => print_json(&call(&socket, "radio_status", Value::Null).await?),
        },
        Command::Diagnostics(flag) => {
            let value = call(&socket, "diagnostics", Value::Null).await?;
            if flag.json {
                print_json(&value)
            } else {
                println!("{}", serde_json::to_string_pretty(&value)?);
                Ok(())
            }
        }
        Command::Recover => print_json(&call(&socket, "recover", Value::Null).await?),
    }
}

async fn connect(socket: &PathBuf) -> Result<UnixStream> {
    UnixStream::connect(socket).await.with_context(|| {
        format!(
            "OmarchyDrop backend is not running (socket: {}). Start omdropd or enable its user service",
            socket.display()
        )
    })
}

async fn call(socket: &PathBuf, method: &str, params: Value) -> Result<Value> {
    let mut stream = connect(socket).await?;
    let request = IpcRequest {
        version: IPC_VERSION,
        id: 1,
        method: method.to_owned(),
        params,
    };
    let mut encoded = serde_json::to_vec(&request)?;
    encoded.push(b'\n');
    stream.write_all(&encoded).await?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let response: IpcResponse =
        serde_json::from_str(&line).context("daemon returned invalid JSON")?;
    if let Some(error) = response.error {
        bail!("{}: {}", error.code, error.message);
    }
    Ok(response.result.unwrap_or(Value::Null))
}

async fn stream_events(socket: &PathBuf) -> Result<()> {
    let mut stream = connect(socket).await?;
    let request = IpcRequest {
        version: IPC_VERSION,
        id: 1,
        method: "events".to_owned(),
        params: Value::Null,
    };
    let mut encoded = serde_json::to_vec(&request)?;
    encoded.push(b'\n');
    stream.write_all(&encoded).await?;
    let mut reader = BufReader::new(stream);
    let mut stdout = tokio::io::stdout();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        stdout.write_all(line.as_bytes()).await?;
        stdout.flush().await?;
    }
}

fn print_status(value: Value, json_output: bool) -> Result<()> {
    if json_output {
        return print_json(&value);
    }
    let snapshot: StatusSnapshot = serde_json::from_value(value)?;
    println!("State: {}", snapshot.state);
    println!("Wi-Fi adapters: {}", snapshot.adapters.len());
    println!(
        "Bluetooth: {}",
        if snapshot.bluetooth.available {
            "available"
        } else {
            "unavailable"
        }
    );
    if let Some(message) = snapshot.message {
        println!("{message}");
    }
    Ok(())
}

fn print_value(value: Value, json_output: bool) -> Result<()> {
    if json_output {
        print_json(&value)
    } else {
        println!("{}", serde_json::to_string_pretty(&value)?);
        Ok(())
    }
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}
