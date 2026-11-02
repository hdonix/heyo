//! A server that speaks Doubao's protocol as Volcengine documents it, on
//! this computer, for the tests and for a trial of Heyo against it
//! (`HEYO_DOUBAO_URL`, which a build for debugging reads): it checks what
//! it is sent as the documentation says it must be, and answers with the
//! sentences it is given, one finished every few packets.

use std::collections::BTreeMap;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::thread::JoinHandle;

use serde_json::{json, Value};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::{Message, WebSocket};

/// What the server does.
#[derive(Clone, Debug)]
pub(crate) enum Behaviour {
    /// Hears these sentences: the first in progress from the first packet
    /// of audio, finished after `every` packets, and so on; all of them
    /// finished at the last packet.
    Hears { sentences: Vec<String>, every: usize },
    /// Refuses the handshake with this HTTP status and body.
    Refuses(u16, &'static str),
    /// Hears the first sentence in progress, then fails after this many
    /// packets with an error packet.
    Fails { after: usize, code: u32, message: &'static str },
}

/// What a client sent, as the server saw it.
#[derive(Debug, Default)]
pub(crate) struct Seen {
    pub headers: BTreeMap<String, String>,
    pub path: String,
    pub request: Value,
    /// The sizes of the audio packets, unzipped, in order.
    pub packets: Vec<usize>,
    /// The last packet was marked as the last.
    pub last: bool,
    /// The client closed the connection.
    pub closed: bool,
    /// What was not as the documentation says.
    pub wrong: Vec<String>,
}

fn gunzip(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes).read_to_end(&mut out).ok()?;
    Some(out)
}

/// A client's packet: kind, flags, how it is written, and its payload,
/// unzipped; or what is wrong with it.
fn client_packet(bytes: &[u8]) -> Result<(u8, u8, u8, Vec<u8>), String> {
    if bytes.len() < 8 {
        return Err(format!("a packet of {} bytes", bytes.len()));
    }
    if bytes[0] != 0x11 {
        return Err(format!("version and header size {:#04x}, not 0x11", bytes[0]));
    }
    if bytes[2] & 0x0f != 1 {
        return Err("not gzipped".into());
    }
    if bytes[3] != 0 {
        return Err("reserved byte not 0".into());
    }
    let size = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if size != bytes.len() - 8 {
        return Err(format!("payload size {size}, but {} bytes follow", bytes.len() - 8));
    }
    let payload = gunzip(&bytes[8..]).ok_or("a payload that is not gzip")?;
    Ok((bytes[1] >> 4, bytes[1] & 0x0f, bytes[2] >> 4, payload))
}

/// A full server response: version 1, header of 4 bytes, a sequence
/// number, JSON gzipped.
pub(crate) fn response(sequence: u32, last: bool, result: &Value) -> Vec<u8> {
    let payload = super::gzip(result.to_string().as_bytes());
    let mut bytes = vec![0x11, 0x90 | if last { 0b0011 } else { 0b0001 }, 0x11, 0];
    bytes.extend_from_slice(&sequence.to_be_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&payload);
    bytes
}

/// An error packet: its code, then its message's size and the message.
pub(crate) fn error(code: u32, message: &str) -> Vec<u8> {
    let mut bytes = vec![0x11, 0xf0, 0x10, 0];
    bytes.extend_from_slice(&code.to_be_bytes());
    bytes.extend_from_slice(&(message.len() as u32).to_be_bytes());
    bytes.extend_from_slice(message.as_bytes());
    bytes
}

/// What it has heard after `packets` packets: the sentences finished so
/// far, and the next one begun.
fn heard(sentences: &[String], every: usize, packets: usize, last: bool) -> Value {
    let finished = if last { sentences.len() } else { (packets / every).min(sentences.len()) };
    let mut utterances: Vec<Value> = sentences[..finished].iter().enumerate().map(|(at, text)| json!({ "text": text, "definite": true, "start_time": at * 1000, "end_time": at * 1000 + 900 })).collect();
    if let Some(next) = sentences.get(finished).filter(|_| !packets.is_multiple_of(every) || packets == 0) {
        let half: String = next.chars().take(next.chars().count() / 2 + 1).collect();
        utterances.push(json!({ "text": half, "definite": false, "start_time": finished * 1000, "end_time": finished * 1000 + 500 }));
    }
    let text: Vec<&str> = utterances.iter().filter_map(|utterance| utterance["text"].as_str()).collect();
    json!({ "audio_info": { "duration": packets * 200 }, "result": { "text": text.concat(), "utterances": utterances } })
}

/// One connection, to its end.
fn serve(stream: TcpStream, behaviour: &Behaviour) -> Seen {
    let mut seen = Seen::default();
    let refusal = match behaviour {
        Behaviour::Refuses(status, body) => Some((*status, *body)),
        _ => None,
    };
    // The callback's type is tungstenite's.
    #[allow(clippy::result_large_err)]
    let looked = |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
        seen.path = request.uri().path().to_owned();
        seen.headers = request.headers().iter().map(|(name, value)| (name.as_str().to_owned(), value.to_str().unwrap_or_default().to_owned())).collect();
        if let Some((status, body)) = refusal {
            let mut refused = ErrorResponse::new(Some(body.to_owned()));
            *refused.status_mut() = tungstenite::http::StatusCode::from_u16(status).unwrap();
            return Err(refused);
        }
        let mut response = response;
        response.headers_mut().insert("X-Tt-Logid", "20260101000000MOCK".parse().unwrap());
        Ok(response)
    };
    let Ok(mut socket) = tungstenite::accept_hdr(stream, looked) else { return seen };
    let (mut sequence, mut packets) = (0u32, 0usize);
    let send = |socket: &mut WebSocket<TcpStream>, bytes: Vec<u8>| drop(socket.send(Message::Binary(bytes.into())));
    loop {
        let message = match socket.read() {
            Ok(message) => message,
            Err(_) => return seen,
        };
        let bytes = match message {
            Message::Binary(bytes) => bytes,
            Message::Close(_) => {
                seen.closed = true;
                let _ = socket.flush();
                return seen;
            }
            _ => continue,
        };
        let (kind, flags, serialization, payload) = match client_packet(&bytes) {
            Ok(packet) => packet,
            Err(wrong) => {
                seen.wrong.push(wrong);
                continue;
            }
        };
        sequence += 1;
        match kind {
            0b0001 => {
                if sequence != 1 {
                    seen.wrong.push("a full client request that is not the first".into());
                }
                if serialization != 1 {
                    seen.wrong.push("a request that is not JSON".into());
                }
                seen.request = serde_json::from_slice(&payload).unwrap_or(Value::Null);
                send(&mut socket, response(sequence, false, &json!({ "audio_info": { "duration": 0 }, "result": { "text": "" } })));
            }
            0b0010 => {
                if sequence == 1 {
                    seen.wrong.push("audio before the full client request".into());
                }
                if serialization != 0 {
                    seen.wrong.push("audio that says it is serialized".into());
                }
                if flags & 0b0001 != 0 {
                    seen.wrong.push("a sequence number the client did not send".into());
                }
                packets += 1;
                seen.packets.push(payload.len());
                let last = flags & 0b0010 != 0;
                seen.last = last;
                match behaviour {
                    Behaviour::Hears { sentences, every } => send(&mut socket, response(sequence, last, &heard(sentences, *every, packets, last))),
                    Behaviour::Fails { after, code, message } if packets >= *after => {
                        send(&mut socket, error(*code, message));
                        let _ = socket.close(None);
                        let _ = socket.flush();
                    }
                    Behaviour::Fails { .. } => send(&mut socket, response(sequence, false, &heard(&["你好世界".into()], 1000, packets, false))),
                    Behaviour::Refuses(..) => {}
                }
            }
            other => seen.wrong.push(format!("a packet of kind {other}")),
        }
    }
}

/// Serves one connection on a port of its own; the address to give the
/// client, and what it saw.
pub(crate) fn once(behaviour: Behaviour) -> (String, JoinHandle<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/api/v3/sauc/bigmodel_async", listener.local_addr().unwrap());
    let served = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve(stream, &behaviour)
    });
    (url, served)
}

/// Serves until it is ended, on `HEYO_DOUBAO_MOCK_PORT` (else 18099), for
/// a trial of Heyo built for debugging with
/// `HEYO_DOUBAO_URL=ws://127.0.0.1:18099/api/v3/sauc/bigmodel_async`:
///
///   cargo test doubao::mock::serves_a_trial -- --ignored --nocapture
#[test]
#[ignore = "a server for a trial, which runs until it is ended"]
fn serves_a_trial() {
    let port = std::env::var("HEYO_DOUBAO_MOCK_PORT").unwrap_or_else(|_| "18099".into());
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();
    eprintln!("mock Doubao on ws://127.0.0.1:{port}/api/v3/sauc/bigmodel_async");
    for stream in listener.incoming().flatten() {
        let sentences = vec!["你好，这是豆包的模拟服务。".to_owned(), "Heyo hears it in the cloud.".to_owned()];
        let seen = serve(stream, &Behaviour::Hears { sentences, every: 8 });
        let key = |name: &str| seen.headers.get(name).map(|value| format!("{} characters", value.len())).unwrap_or_else(|| "none".into());
        eprintln!(
            "connection: X-Api-Key {}, X-Api-App-Key {}, X-Api-Access-Key {}, resource {:?}, connect id {:?}; {} packets {:?}, last {}, closed {}, wrong {:?}; request {}",
            key("x-api-key"),
            key("x-api-app-key"),
            key("x-api-access-key"),
            seen.headers.get("x-api-resource-id"),
            seen.headers.get("x-api-connect-id"),
            seen.packets.len(),
            seen.packets.iter().take(3).collect::<Vec<_>>(),
            seen.last,
            seen.closed,
            seen.wrong,
            seen.request
        );
    }
}
