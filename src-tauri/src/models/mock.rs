//! A server for the tests of the adapters to ask: on a port of its own on
//! this computer, answering each request as the test says, keeping what
//! it was asked, and able to keep a stream open until the asker goes away.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use tauri::async_runtime::block_on;

/// A request as the server read it.
#[derive(Debug, Clone)]
pub struct Asked {
    pub method: String,
    pub path: String,
    /// Header names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Asked {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

/// How the server answers a request.
pub enum Reply {
    /// A status and the whole body.
    Whole(u16, String),
    /// A stream of events that begins with this and then goes quiet, until
    /// the asker hangs up.
    Stalls(String),
}

impl From<(u16, String)> for Reply {
    fn from((status, body): (u16, String)) -> Reply {
        Reply::Whole(status, body)
    }
}

pub struct Server {
    port: u16,
    asked: Arc<Mutex<Vec<Asked>>>,
    /// How many streams that stalled the asker has hung up on.
    hung_up: Arc<Mutex<usize>>,
}

impl Server {
    pub fn start<R: Into<Reply>>(answer: impl Fn(&Asked) -> R + Send + Sync + 'static) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (asked, hung_up) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(0)));
        let (keeping, counting) = (asked.clone(), hung_up.clone());
        let answer = Arc::new(answer);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (keeping, counting, answer) = (keeping.clone(), counting.clone(), answer.clone());
                std::thread::spawn(move || serve(stream, &keeping, &counting, |request| answer(request).into()));
            }
        });
        Server { port, asked, hung_up }
    }

    /// The address of `path` on this server.
    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    pub fn hung_up(&self) -> usize {
        *self.hung_up.lock().unwrap()
    }
}

fn serve(stream: TcpStream, asked: &Mutex<Vec<Asked>>, hung_up: &Mutex<usize>, answer: impl Fn(&Asked) -> Reply) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or_default().to_owned(), parts.next().unwrap_or_default().to_owned());
    let mut headers = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        let Some((name, value)) = line.trim_end().split_once(':') else { break };
        headers.push((name.trim().to_lowercase(), value.trim().to_owned()));
    }
    let length = headers.iter().find(|(n, _)| n == "content-length").map_or(0, |(_, v)| v.parse().unwrap_or(0));
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let request = Asked { method, path, headers, body: String::from_utf8(body).unwrap() };
    asked.lock().unwrap().push(request.clone());
    let mut stream = stream;
    match answer(&request) {
        Reply::Whole(status, body) => {
            let kind = if body.starts_with("data:") || body.starts_with("event:") { "text/event-stream" } else { "application/json" };
            let _ = write!(stream, "HTTP/1.1 {status} X\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
        }
        Reply::Stalls(start) => {
            let _ = write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{start}");
            let _ = stream.flush();
            // Keep-alives until a write fails: the asker has gone.
            stream.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
            loop {
                std::thread::sleep(Duration::from_millis(50));
                if stream.write_all(b": keep-alive\n\n").and_then(|_| stream.flush()).is_err() {
                    break;
                }
                let mut byte = [0];
                if matches!(stream.read(&mut byte), Ok(0)) {
                    break;
                }
            }
            *hung_up.lock().unwrap() += 1;
        }
    }
}
