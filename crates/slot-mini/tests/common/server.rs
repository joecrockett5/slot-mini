#![allow(dead_code)]

//! A tiny HTTP/1.1 server for label service tests: fixed files, an ETag on
//! index.json, and a log of what was asked. Never touches the internet.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub struct Server {
    pub base: String,
    pub log: Arc<Mutex<Vec<String>>>,
}

pub const ETAG: &str = "\"v1\"";

impl Server {
    /// Serve `files` (path -> bytes). `index.json` gets an ETag and answers
    /// 304 when asked with it.
    pub fn start(files: HashMap<String, Vec<u8>>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let log = Arc::new(Mutex::new(Vec::new()));
        let seen = log.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let path = line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .trim_start_matches('/')
                    .to_string();
                let mut if_none_match = None;
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                    if let Some((k, v)) = h.split_once(':') {
                        if k.eq_ignore_ascii_case("if-none-match") {
                            if_none_match = Some(v.trim().to_string());
                        }
                    }
                }
                let tagged = if_none_match.is_some();
                seen.lock().unwrap().push(format!(
                    "{path}{}",
                    if tagged { " (if-none-match)" } else { "" }
                ));
                let reply = match files.get(&path) {
                    Some(_) if path == "index.json" && if_none_match.as_deref() == Some(ETAG) => {
                        "HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".as_bytes().to_vec()
                    }
                    Some(body) => {
                        let mut r = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: {ETAG}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        r.extend_from_slice(body);
                        r
                    }
                    None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".as_bytes().to_vec(),
                };
                let _ = stream.write_all(&reply);
            }
        });
        Server { base, log }
    }

    pub fn asked(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}
