// The Jolly Executioner
// by @singe
// A simple execution proxy with some security ideas

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::engine::Engine as _;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Write;
use std::process::Stdio;
use tempfile::NamedTempFile;
use tokio::process::Command;
use warp::{
    http::{Response, StatusCode},
    reject::Reject,
    Filter,
};

#[derive(Deserialize, Serialize, Clone)]
struct Config {
    routes: Vec<RouteConfig>,
}

#[derive(Deserialize, Serialize, Clone)]
struct RouteConfig {
    path: String,
    command: String,
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FileInput {
    filename: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct Input {
    files: Option<Vec<FileInput>>,
    params: Option<Vec<String>>,
}

#[derive(Debug)]
struct CommandError {
    message: String,
}

impl Reject for CommandError {}

async fn handle_and_execute_files(
    input: Input,
    command: &str,
    command_options: &[String],
) -> Result<Response<Bytes>, warp::Rejection> {
    let mut cmd = Command::new(command);
    let mut file_paths = Vec::new();
    let mut stdin_data = None;

    // Decode and write files to temporary locations
    if let Some(files) = input.files {
        for file in files {
            let file_data = BASE64.decode(&file.data).map_err(|_| {
                warp::reject::custom(CommandError {
                    message: format!("Failed to decode file: {}", file.filename),
                })
            })?;

            let mut temp_file = NamedTempFile::new().map_err(|_| {
                warp::reject::custom(CommandError {
                    message: format!("Failed to create temp file for file: {}", file.filename),
                })
            })?;
            temp_file.write_all(&file_data).map_err(|_| {
                warp::reject::custom(CommandError {
                    message: format!("Failed to write file: {}", file.filename),
                })
            })?;

            file_paths.push(temp_file);
        }
    }

    let mut args = Vec::new();
    let mut file_iter = file_paths.iter();
    let mut params_iter = input.params.iter().flatten();
    for option in command_options.iter() {
        match option.as_str() {
            "%s" => {
                if let Some(file_path) = file_iter.next() {
                    stdin_data = Some(std::fs::read(file_path.path().to_str().unwrap()).map_err(
                        |_| {
                            warp::reject::custom(CommandError {
                                message: "Failed to read file for stdin".to_string(),
                            })
                        },
                    )?);
                }
            }
            "%f" => {
                if let Some(file_path) = file_iter.next() {
                    args.push(file_path.path().to_str().unwrap().to_string());
                }
            }
            "%p" => {
                if let Some(param) = params_iter.next() {
                    args.push(param.clone());
                }
            }
            _ => args.push(option.to_string()),
        }
    }
    
    println!("Executing command: {command}, with arguments {:?}",&args);

    let mut child = cmd
        .args(&args)
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            warp::reject::custom(CommandError {
                message: format!("Failed to start command: {}", e),
            })
        })?;

    if let Some(data) = stdin_data {
        // Write input to stdin of the child process
        if let Some(mut stdin) = child.stdin.take() {
            tokio::io::AsyncWriteExt::write_all(&mut stdin, &data)
                .await
                .map_err(|e| {
                    warp::reject::custom(CommandError {
                        message: format!("Failed to write to stdin: {}", e),
                    })
                })?;
        }
    }

    let output = child.wait_with_output().await.map_err(|e| {
        warp::reject::custom(CommandError {
            message: format!("Failed to read stdout/stderr: {}", e),
        })
    })?;

    // Base64 encode stdout and stderr
    let encoded_stdout = BASE64.encode(&output.stdout);
    let encoded_stderr = BASE64.encode(&output.stderr);

    let response_json = json!({
        "stdout": encoded_stdout,
        "stderr": encoded_stderr,
        "exit_code": output.status.code().unwrap_or_default()
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Bytes::from(response_json.to_string()))
        .unwrap())
}

fn generate_curl_commands(config: &Config) -> String {
    let mut output = String::new();
    for route in &config.routes {
        let mut data = String::new();
        let mut has_files = false;
        let mut has_params = false;
        let mut files_data = String::from("  \"files\": [\n");
        let mut params_data = String::from("  \"params\": [\n");
        let mut file_count = 1;

        for arg in &route.args {
            match arg.as_str() {
                "%f" | "%s" => {
                    files_data += &format!("    {{\"filename\": \"file{}.txt\", \"data\": \"'\"$(base64 -w0 file{}.txt)\"'\"}},\n", file_count, file_count);
                    file_count += 1;
                    has_files = true;
                },
                "%p" => {
                    params_data += &format!("    \"param{}\",\n", file_count);
                    file_count += 1;
                    has_params = true;
                },
                _ => {}
            }
        }

        if has_files {
            files_data.pop(); files_data.pop(); // Remove the last comma and newline
            files_data += "\n  ],\n";
        } else {
            files_data.clear();
        }

        if has_params {
            params_data.pop(); params_data.pop(); // Remove the last comma and newline
            params_data += "\n  ]\n";
        } else {
            params_data.clear();
        }

        data += "{\n";
        if has_files { data += &files_data; }
        if has_params { data += &params_data; }
        if data.ends_with(",\n") { data.pop(); data.pop(); } // Clean up any trailing commas
        data += "}";

        output.push_str(&format!("curl -X POST http://localhost:3030/{} \\\n     -H \"Content-Type: application/json\" \\\n     -d '{}'\n", route.path, data));
    }
    output
}

fn load_config(file_path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let contents = std::fs::read_to_string(file_path)?;
    let config: Config = serde_json::from_str(&contents)?;
    Ok(config)
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    // Load the configuration
    let config = load_config("config.json").expect("Failed to load config");

    // Start with a base route that matches nothing
    let base = warp::any()
        .and(warp::path("help"))
        .map(|| {
            let config = load_config("config.json").expect("Failed to load config");
            let help = generate_curl_commands(&config);
            Response::builder()
                .status(200)
                .body(help.into())
                .unwrap()
        })
        .boxed();

    let routes = config
        .routes
        .into_iter()
        .map(|route_config| {
            let path = route_config.path;
            let command = route_config.command;
            let args = route_config.args;
            println!(
                "Added Route -  path: {}, command: {}, args: {:?}",
                path, command, args
            );

            warp::post()
            .and(warp::path(path))
            .and(warp::body::json())
            .and_then(move |input: Input| {
                let command_clone = command.clone();
                let args_clone = args.clone();

                async move {
                    handle_and_execute_files(input, &command_clone, &args_clone).await
                }
            })
            .boxed()
        })
        .fold(base, |routes, route| routes.or(route).unify().boxed());

    // Serve the API
    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
}

