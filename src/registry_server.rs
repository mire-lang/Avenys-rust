use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tiny_http::{Header, Response, Server};

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut port: u16 = 9670;
    let mut dir = PathBuf::from("packages");

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                i += 1;
                if i < args.len() {
                    port = args[i].parse().unwrap_or(9670);
                }
            }
            "--dir" | "-d" => {
                i += 1;
                if i < args.len() {
                    dir = PathBuf::from(&args[i]);
                }
            }
            _ => {
                eprintln!("Usage: registry-server [--port <port>] [--dir <packages-dir>]");
                return;
            }
        }
        i += 1;
    }

    if !dir.exists() {
        fs::create_dir_all(&dir).unwrap_or_else(|e| {
            eprintln!("Cannot create packages dir: {}", e);
            std::process::exit(1);
        });
    }

    let addr = format!("0.0.0.0:{}", port);
    let server = Server::http(&addr).unwrap_or_else(|e| {
        eprintln!("Cannot bind to {}: {}", addr, e);
        std::process::exit(1);
    });

    eprintln!("Registry server listening on http://{}", addr);
    eprintln!("Serving packages from: {}", dir.display());

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let path = url.trim_start_matches('/');

        if path.is_empty() || path == "/" {
            respond_index(&dir, request);
        } else if let Some(rest) = path.strip_prefix("packages/") {
            respond_package(&dir, rest, request);
        } else {
            let _ = request.respond(Response::from_string("404 not found").with_status_code(404));
        }
    }
}

fn respond_index(dir: &Path, request: tiny_http::Request) {
    let mut packages = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let meta_path = entry.path().join("meta.json");
                let version = if meta_path.exists() {
                    read_meta_field(&meta_path, "version").unwrap_or_else(|| "unknown".to_string())
                } else {
                    "unknown".to_string()
                };
                packages.push(format!("{}@{}", name, version));
            }
        }
    }

    let body = if packages.is_empty() {
        "no packages".to_string()
    } else {
        packages.join("\n")
    };

    let _ = request.respond(
        Response::from_string(body)
            .with_header(Header::from_bytes("Content-Type", "text/plain").unwrap()),
    );
}

fn respond_package(dir: &Path, rest: &str, request: tiny_http::Request) {
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    if parts.is_empty() {
        let _ = request.respond(Response::from_string("bad request").with_status_code(400));
        return;
    }

    let pkg_name = parts[0];
    let pkg_dir = dir.join(pkg_name);

    if !pkg_dir.exists() {
        let _ = request.respond(Response::from_string("package not found").with_status_code(404));
        return;
    }

    if parts.len() == 1 {
        let meta_path = pkg_dir.join("meta.json");
        let version =
            read_meta_field(&meta_path, "version").unwrap_or_else(|| "unknown".to_string());
        let sha256 = read_meta_field(&meta_path, "sha256").unwrap_or_else(|| "-".to_string());

        let body = format!("{}@{}\nsha256: {}\n", pkg_name, version, sha256);
        let _ = request.respond(
            Response::from_string(body)
                .with_header(Header::from_bytes("Content-Type", "text/plain").unwrap()),
        );
        return;
    }

    let sub = parts[1];

    if sub == "meta" {
        serve_file(&pkg_dir.join("meta.json"), "application/json", request);
    } else {
        let ct = if sub.ends_with(".tar.gz") {
            "application/gzip"
        } else {
            "application/octet-stream"
        };
        serve_file(&pkg_dir.join(sub), ct, request);
    }
}

fn serve_file(path: &Path, content_type: &str, request: tiny_http::Request) {
    if !path.exists() {
        let _ = request.respond(Response::from_string("file not found").with_status_code(404));
        return;
    }

    match fs::read(path) {
        Ok(data) => {
            let response = Response::from_data(data)
                .with_header(Header::from_bytes("Content-Type", content_type).unwrap());
            let _ = request.respond(response);
        }
        Err(e) => {
            let _ = request
                .respond(Response::from_string(format!("read error: {}", e)).with_status_code(500));
        }
    }
}

fn read_meta_field(path: &Path, field: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let line = line.trim().trim_matches(|c| c == '"' || c == ',');
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().trim_matches('"');
            let value = value.trim().trim_matches('"').trim_end_matches(',');
            if key == field {
                return Some(value.to_string());
            }
        }
    }
    None
}
