//! Doubao's streaming speech recognition (Volcengine, 豆包流式语音识别):
//! the audio of a recording goes to Volcengine over a WebSocket as it is
//! recorded, and what it hears comes back, the sentence in progress and
//! each finished one.
//!
//! As Volcengine documents it ("大模型流式语音识别API",
//! https://docs.volcengine.com/docs/6561/1354869): the optimised
//! bidirectional endpoint (`bigmodel_async`), which answers only when the
//! result changes; the key in the headers of the handshake, with the
//! resource it is for; then packets of a binary protocol of its own (a
//! 4-byte header, a big-endian size, a gzipped payload): one request with
//! the parameters in JSON, the audio as 16-bit PCM in packets of 200 ms,
//! the last one marked. Each answer has all the sentences so far
//! (`result_type: full`, `show_utterances`); a sentence is finished when
//! it is `definite`, which on this endpoint only the second pass says
//! (`enable_nonstream`, with its pause detection at 800 ms of silence).
//!
//! The connection is a thread's own: it sends what the recording gives it
//! and says what it hears as it comes. A recording that is discarded
//! closes it.

use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::{HeaderValue, Response};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use super::sentences::between;
use super::{Failure, Recognized, Recognizer, Say, SAMPLE_RATE};
use crate::i18n::marked;

/// Where Volcengine listens: the bidirectional endpoint, optimised.
const ENDPOINT: &str = "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async";

/// The provider's models: the resources a key can be for
/// (`X-Api-Resource-Id`), model 2.0 or 1.0, paid by the hour of audio or
/// by concurrent connections, as Volcengine names them. The first is the
/// one a reference without a model means.
pub(crate) const RESOURCES: &[(&str, &str)] = &[
    ("volc.seedasr.sauc.duration", marked("Doubao Streaming ASR 2.0, Hourly")),
    ("volc.seedasr.sauc.concurrent", marked("Doubao Streaming ASR 2.0, Concurrent")),
    ("volc.bigasr.sauc.duration", marked("Doubao Streaming ASR 1.0, Hourly")),
    ("volc.bigasr.sauc.concurrent", marked("Doubao Streaming ASR 1.0, Concurrent")),
];

// --- The provider, as the settings declare it (`models::registry`) ---------

/// The consoles a key can be from: value, title.
pub(crate) const CONSOLES: &[(&str, &str)] = &[("new", marked("New Console: API Key")), ("old", marked("Old Console: App ID and Access Token"))];

/// The fields of a Doubao provider's configuration (`registry::Provider::config`).
pub(crate) const CONSOLE: &str = "console";
pub(crate) const API_KEY: &str = "apiKey";
pub(crate) const APP_ID: &str = "appId";
pub(crate) const ACCESS_TOKEN: &str = "accessToken";
pub(crate) const REMOVE_FILLERS: &str = "removeFillers";

/// The audio of a packet: 200 ms, which the documentation says the
/// bidirectional mode works best with.
const PACKET: usize = SAMPLE_RATE / 5;

/// How long a connection may take, and how long the last answer.
const CONNECTING: Duration = Duration::from_secs(10);
const LAST_ANSWER: Duration = Duration::from_secs(10);

/// How long a read waits for an answer before what is to be sent is looked
/// at again.
const POLL: Duration = Duration::from_millis(20);

/// How many tokens of hotwords the bidirectional mode takes.
const HOTWORD_TOKENS: usize = 100;

/// Where the address of the service can be another, for the tests and a
/// trial against a server of one's own: `HEYO_DOUBAO_URL`, which only a
/// build for debugging reads.
fn endpoint() -> String {
    #[cfg(debug_assertions)]
    if let Some(url) = std::env::var("HEYO_DOUBAO_URL").ok().filter(|url| !url.is_empty()) {
        return url;
    }
    ENDPOINT.into()
}

/// The key, from either of Volcengine's consoles.
#[derive(Clone, PartialEq)]
pub(crate) enum Key {
    /// The new console's API key (`X-Api-Key`).
    Api(String),
    /// The old console's App ID and Access Token.
    App { id: String, token: String },
}

/// Everything a recording needs said to Volcengine.
#[derive(Clone, PartialEq)]
pub(crate) struct Config {
    pub key: Key,
    pub resource: String,
    /// Smoothing (`enable_ddc`): filler words, stutters and repetitions
    /// taken out.
    pub remove_fillers: bool,
    /// Words to be heard as written: the vocabulary's.
    pub hotwords: Vec<String>,
}

// Never with the key in it: it may be printed.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let console = match self.key {
            Key::Api(_) => "API key",
            Key::App { .. } => "App ID and Access Token",
        };
        f.debug_struct("Config").field("key", &console).field("resource", &self.resource).field("remove_fillers", &self.remove_fillers).field("hotwords", &self.hotwords.len()).finish()
    }
}

impl Config {
    /// The config a provider of Doubao's is set up with, for the model
    /// (resource) `model`, an empty one being the first, with the words of
    /// `vocabulary`. A key the console needs and has not been given is what
    /// fails (`doubao-no-key`).
    pub(crate) fn of(provider: &crate::models::registry::Provider, model: &str, vocabulary: &crate::dictation::vocabulary::Vocabulary) -> Result<Config, Failure> {
        let key = Config::key(provider.get(CONSOLE), provider.get(API_KEY), provider.get(APP_ID), provider.get(ACCESS_TOKEN)).ok_or_else(|| Failure::new("doubao-no-key"))?;
        let resource = match model {
            "" => RESOURCES[0].0,
            model => RESOURCES.iter().map(|(id, _)| *id).find(|id| *id == model).ok_or_else(|| Failure::failed(format!("Doubao has no model {model}")))?,
        };
        Ok(Config { key, resource: resource.into(), remove_fillers: provider.get(REMOVE_FILLERS) == "true", hotwords: hotwords(vocabulary) })
    }

    /// Whether a provider has all of the key its console needs.
    pub(crate) fn keyed(provider: &crate::models::registry::Provider) -> bool {
        Config::key(provider.get(CONSOLE), provider.get(API_KEY), provider.get(APP_ID), provider.get(ACCESS_TOKEN)).is_some()
    }

    /// The key of that console, where all of it is given.
    fn key(console: &str, api_key: &str, app_id: &str, access_token: &str) -> Option<Key> {
        match (console, api_key.trim(), app_id.trim(), access_token.trim()) {
            ("old", _, "", _) | ("old", _, _, "") => None,
            ("old", _, id, token) => Some(Key::App { id: id.into(), token: token.into() }),
            (_, "", ..) => None,
            (_, key, ..) => Some(Key::Api(key.into())),
        }
    }

    /// What is asked of the service: the audio's format and how it is to
    /// be recognized.
    fn request(&self) -> Value {
        let mut request = json!({
            "model_name": "bigmodel",
            "enable_itn": true,
            "enable_punc": true,
            "enable_ddc": self.remove_fillers,
            "enable_nonstream": true,
            "show_utterances": true,
            "result_type": "full",
        });
        if !self.hotwords.is_empty() {
            let words: Vec<Value> = self.hotwords.iter().map(|word| json!({ "word": word })).collect();
            request["corpus"] = json!({ "context": json!({ "hotwords": words }).to_string() });
        }
        json!({
            "audio": { "format": "pcm", "codec": "raw", "rate": SAMPLE_RATE, "bits": 16, "channel": 1 },
            "request": request,
        })
    }
}

/// The words of the vocabulary as they are to be written, as many as the
/// service takes: tokens are counted as a character of Chinese or Japanese
/// each, and a word of anything else each, which is never fewer than the
/// service counts.
fn hotwords(vocabulary: &crate::dictation::vocabulary::Vocabulary) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut tokens = 0;
    let written = vocabulary.terms.iter().chain(vocabulary.replacements.iter().map(|(_, written)| written));
    for word in written {
        if word.trim().is_empty() || words.contains(word) {
            continue;
        }
        let cost = word.chars().filter(|c| !c.is_ascii()).count() + word.split(|c: char| !c.is_ascii_alphanumeric()).filter(|part| !part.is_empty()).count();
        if tokens + cost.max(1) > HOTWORD_TOKENS {
            break;
        }
        tokens += cost.max(1);
        words.push(word.clone());
    }
    words
}

// --- The protocol --------------------------------------------------------

/// The kinds of packet (the high half of the header's second byte).
const FULL_CLIENT_REQUEST: u8 = 0b0001;
const AUDIO_ONLY_REQUEST: u8 = 0b0010;
const FULL_SERVER_RESPONSE: u8 = 0b1001;
const ERROR_RESPONSE: u8 = 0b1111;

/// Flags of a packet (the low half): a sequence number follows the
/// header; the packet is the last.
const HAS_SEQUENCE: u8 = 0b0001;
const LAST: u8 = 0b0010;

const JSON: u8 = 0b0001;
const RAW: u8 = 0b0000;
const GZIP: u8 = 0b0001;

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).expect("writing to memory");
    encoder.finish().expect("writing to memory")
}

fn gunzip(bytes: &[u8]) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes).read_to_end(&mut out)?;
    Ok(out)
}

/// A packet the client sends: version 1 and a header of 4 bytes, its
/// kind and flags, how its payload is written, then the payload's size and
/// the payload, gzipped.
pub(crate) fn packet(kind: u8, flags: u8, serialization: u8, payload: &[u8]) -> Vec<u8> {
    let payload = gzip(payload);
    let mut packet = vec![0x11, kind << 4 | flags, serialization << 4 | GZIP, 0];
    packet.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    packet.extend_from_slice(&payload);
    packet
}

/// The audio, as the service takes it: 16-bit little-endian samples.
fn pcm(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|sample| (((sample.clamp(-1.0, 1.0)) * i16::MAX as f32) as i16).to_le_bytes()).collect()
}

/// What the service sends.
#[derive(Debug, PartialEq)]
pub(crate) enum Answer {
    /// What it heard so far; `last` once it has heard everything.
    Heard { result: Value, last: bool },
    Error { code: u32, message: String },
}

fn malformed(what: &str) -> Failure {
    Failure::new("doubao-error").with("detail", format!("an answer that cannot be read ({what})"))
}

fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Result<&'a [u8], Failure> {
    if bytes.len() < count {
        return Err(malformed("too short"));
    }
    let (taken, rest) = bytes.split_at(count);
    *bytes = rest;
    Ok(taken)
}

fn number(bytes: &mut &[u8]) -> Result<u32, Failure> {
    Ok(u32::from_be_bytes(take(bytes, 4)?.try_into().expect("four bytes")))
}

/// Reads a packet the service sent.
pub(crate) fn answer(mut bytes: &[u8]) -> Result<Answer, Failure> {
    let header = take(&mut bytes, 4)?.to_vec();
    // Header extensions, where the header says it is longer.
    take(&mut bytes, ((header[0] & 0x0f) as usize * 4).saturating_sub(4))?;
    let (kind, flags, compression) = (header[1] >> 4, header[1] & 0x0f, header[2] & 0x0f);
    let payload = |bytes: &mut &[u8]| -> Result<Vec<u8>, Failure> {
        let size = number(bytes)? as usize;
        let payload = take(bytes, size)?;
        if compression == GZIP {
            gunzip(payload).map_err(|e| malformed(&e.to_string()))
        } else {
            Ok(payload.to_vec())
        }
    };
    match kind {
        FULL_SERVER_RESPONSE => {
            if flags & HAS_SEQUENCE != 0 {
                number(&mut bytes)?;
            }
            let payload = payload(&mut bytes)?;
            let result = if payload.is_empty() { Value::Null } else { serde_json::from_slice(&payload).map_err(|e| malformed(&e.to_string()))? };
            Ok(Answer::Heard { result, last: flags & LAST != 0 })
        }
        ERROR_RESPONSE => {
            let code = number(&mut bytes)?;
            let message = String::from_utf8_lossy(&payload(&mut bytes)?).into_owned();
            Ok(Answer::Error { code, message })
        }
        other => Err(malformed(&format!("a packet of kind {other}"))),
    }
}

// --- What it heard -------------------------------------------------------

/// The sentences of a recording as the service says them, said on as
/// `Recognized`: each finished one once, and the one in progress when it
/// changes.
struct Heard {
    say: Say,
    /// Everything finished, joined.
    all: String,
    /// How many of the service's sentences are said as finished.
    finished: usize,
    partial: String,
}

impl Heard {
    fn new(say: Say) -> Heard {
        Heard { say, all: String::new(), finished: 0, partial: String::new() }
    }

    fn sentence(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let sentence = format!("{}{text}", between(&self.all, text));
        self.all.push_str(&sentence);
        self.partial.clear();
        (self.say)(Recognized::Sentence(sentence));
    }

    /// An answer: its result, with every sentence so far (`utterances`).
    /// The last answer finishes whatever is still in progress.
    fn heard(&mut self, result: &Value, last: bool) {
        // The documentation shows the result as an object and calls it a
        // list: either.
        let result = result.get("result").map(|result| if result.is_array() { &result[0] } else { result }).unwrap_or(&Value::Null);
        let utterances = result["utterances"].as_array().map(Vec::as_slice).unwrap_or_default();
        let mut index = self.finished;
        while let Some(utterance) = utterances.get(index).filter(|utterance| utterance["definite"] == true) {
            self.sentence(utterance["text"].as_str().unwrap_or_default());
            index += 1;
        }
        self.finished = index.max(self.finished);
        let rest: Vec<&str> = utterances.iter().skip(self.finished).filter_map(|utterance| utterance["text"].as_str()).map(str::trim).filter(|text| !text.is_empty()).collect();
        if last {
            for text in rest {
                self.sentence(text);
            }
            // Heard without being parted into sentences.
            if self.all.is_empty() {
                self.sentence(result["text"].as_str().unwrap_or_default());
            }
            return;
        }
        let partial = rest.join(" ");
        if partial != self.partial {
            self.partial = partial;
            let said = if self.partial.is_empty() { String::new() } else { format!("{}{}", between(&self.all, &self.partial), self.partial) };
            (self.say)(Recognized::Partial(said));
        }
    }

    /// The recording failed: what was being said is kept as it was last
    /// heard, and the failure said.
    fn failed(&mut self, failure: &Failure) {
        let partial = std::mem::take(&mut self.partial);
        self.sentence(&partial);
        (self.say)(Recognized::Failed(failure.clone()));
    }
}

// --- The connection ------------------------------------------------------

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

/// What the recording gives the connection.
enum Out {
    Audio(Vec<u8>),
    /// The last of the audio: the recording is over.
    Last(Vec<u8>),
}

fn offline(detail: impl std::fmt::Display) -> Failure {
    Failure::new("doubao-unreachable").with("detail", detail)
}

/// What a handshake that was answered with an HTTP status says.
fn refused(response: &Response<Option<Vec<u8>>>) -> Failure {
    let body = response.body().as_deref().map(String::from_utf8_lossy).unwrap_or_default();
    let detail = format!("HTTP {} {}", response.status().as_u16(), body.trim()).trim().to_owned();
    let code = match response.status().as_u16() {
        401 => "doubao-key",
        403 => "doubao-denied",
        429 => "doubao-busy",
        _ => "doubao-error",
    };
    if let Some(logid) = response.headers().get("X-Tt-Logid").and_then(|id| id.to_str().ok()) {
        eprintln!("heyo: dictation: Doubao refused the connection, logid {logid}");
    }
    Failure::new(code).with("detail", detail)
}

/// What an error packet says, by its code (the documentation's list).
fn server_error(code: u32, message: &str) -> Failure {
    let kind = match code {
        55000031 => "doubao-busy",
        _ => "doubao-error",
    };
    Failure::new(kind).with("detail", format!("{code}: {}", message.trim()))
}

/// An id for the connection (`X-Api-Connect-Id`), as a random UUID.
fn connect_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let half = || {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
        hasher.finish()
    };
    let bits = (half() as u128) << 64 | half() as u128;
    let bits = bits & !(0xf000 << 64) | (0x4000 << 64);
    let bits = bits & !(0xc000 << 48) | (0x8000 << 48);
    let hex = format!("{bits:032x}");
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// TLS as reqwest has it: rustls with ring, certificates checked by the
/// system's own verifier.
fn tls() -> Result<Arc<rustls::ClientConfig>, Failure> {
    use rustls_platform_verifier::BuilderVerifierExt;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(|builder| builder.with_platform_verifier())
        .map_err(|e| offline(format!("TLS: {e}")))?
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// Connects, with the key in the handshake's headers.
fn connect(url: &str, config: &Config) -> Result<Socket, Failure> {
    let mut request = url.into_client_request().map_err(Failure::failed)?;
    let headers = request.headers_mut();
    let value = |text: &str| HeaderValue::from_str(text).map_err(|_| Failure::new("doubao-key").with("detail", "the key has characters a key cannot have"));
    match &config.key {
        Key::Api(key) => drop(headers.insert("X-Api-Key", value(key)?)),
        Key::App { id, token } => {
            headers.insert("X-Api-App-Key", value(id)?);
            headers.insert("X-Api-Access-Key", value(token)?);
        }
    }
    headers.insert("X-Api-Resource-Id", value(&config.resource)?);
    headers.insert("X-Api-Connect-Id", value(&connect_id())?);
    let uri = request.uri();
    let host = uri.host().unwrap_or_default().trim_matches(['[', ']']).to_owned();
    let port = uri.port_u16().unwrap_or(if uri.scheme_str() == Some("ws") { 80 } else { 443 });
    let addresses: Vec<_> = (host.as_str(), port).to_socket_addrs().map_err(|e| offline(format!("{host}: {e}")))?.collect();
    let mut tried = None;
    let stream = addresses.iter().find_map(|address| TcpStream::connect_timeout(address, CONNECTING).map_err(|e| tried = Some(e)).ok());
    let stream = stream.ok_or_else(|| offline(tried.map_or_else(|| format!("{host}: no address"), |e| format!("{host}: {e}"))))?;
    stream.set_read_timeout(Some(CONNECTING)).and_then(|_| stream.set_write_timeout(Some(CONNECTING))).map_err(offline)?;
    stream.set_nodelay(true).map_err(offline)?;
    let connector = (uri.scheme_str() == Some("wss")).then(tls).transpose()?.map(tungstenite::Connector::Rustls);
    let (socket, _) = tungstenite::client_tls_with_config(request, stream, None, connector.or(Some(tungstenite::Connector::Plain))).map_err(|e| match e {
        tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response)) => refused(&response),
        tungstenite::HandshakeError::Failure(e) => offline(e),
        tungstenite::HandshakeError::Interrupted(_) => offline("the handshake took too long"),
    })?;
    // From now on a read waits only a moment: what is to be sent is not
    // kept waiting for an answer.
    let stream = match socket.get_ref() {
        MaybeTlsStream::Plain(stream) => stream,
        MaybeTlsStream::Rustls(stream) => stream.get_ref(),
        _ => unreachable!("only the two connectors given"),
    };
    stream.set_read_timeout(Some(POLL)).map_err(offline)?;
    Ok(socket)
}

fn send(socket: &mut Socket, packet: Vec<u8>) -> Result<(), Failure> {
    socket.send(Message::Binary(packet.into())).map_err(offline)
}

/// The connection of one recording, until it ends: Ok when the service
/// has said everything it heard, after the last of the audio; an error if
/// it failed before. Gives up when nobody is sending any more.
fn session(url: &str, config: &Config, audio: &Receiver<Out>, heard: &mut Heard) -> Result<(), Failure> {
    let mut socket = connect(url, config)?;
    let request = serde_json::to_vec(&config.request()).expect("JSON");
    send(&mut socket, packet(FULL_CLIENT_REQUEST, 0, JSON, &request))?;
    let mut deadline = None;
    loop {
        loop {
            match audio.try_recv() {
                Ok(Out::Audio(pcm)) => send(&mut socket, packet(AUDIO_ONLY_REQUEST, 0, RAW, &pcm))?,
                Ok(Out::Last(pcm)) => {
                    send(&mut socket, packet(AUDIO_ONLY_REQUEST, LAST, RAW, &pcm))?;
                    deadline = Some(Instant::now() + LAST_ANSWER);
                }
                Err(TryRecvError::Empty) => break,
                // The recording was discarded.
                Err(TryRecvError::Disconnected) if deadline.is_none() => {
                    let _ = socket.close(None);
                    let _ = socket.flush();
                    return Err(Failure::new("cancelled"));
                }
                Err(TryRecvError::Disconnected) => break,
            }
        }
        match socket.read() {
            Ok(Message::Binary(bytes)) => match answer(&bytes)? {
                Answer::Heard { result, last } => {
                    heard.heard(&result, last);
                    if last {
                        let _ = socket.close(None);
                        let _ = socket.flush();
                        return Ok(());
                    }
                }
                Answer::Error { code, message } => return Err(server_error(code, &message)),
            },
            Ok(Message::Close(frame)) => {
                let said = frame.map(|frame| format!("{} {}", u16::from(frame.code), frame.reason.as_str())).unwrap_or_default();
                return Err(Failure::new("doubao-error").with("detail", format!("the connection was closed {said}").trim()));
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(offline(e)),
        }
        if deadline.is_some_and(|deadline| Instant::now() > deadline) {
            return Err(offline("no answer after the end of the recording"));
        }
    }
}

/// Whether the key and the resource are accepted, at little cost: a
/// connection, the request, and the service's first answer to it, with
/// no audio. Takes up to `CONNECTING` twice over; for a thread that can
/// wait.
pub(crate) fn check(config: &Config) -> Result<(), Failure> {
    check_at(&endpoint(), config)
}

fn check_at(url: &str, config: &Config) -> Result<(), Failure> {
    let mut socket = connect(url, config)?;
    let request = serde_json::to_vec(&config.request()).expect("JSON");
    send(&mut socket, packet(FULL_CLIENT_REQUEST, 0, JSON, &request))?;
    let deadline = Instant::now() + CONNECTING;
    let answered = loop {
        match socket.read() {
            Ok(Message::Binary(bytes)) => match answer(&bytes)? {
                Answer::Heard { .. } => break Ok(()),
                Answer::Error { code, message } => break Err(server_error(code, &message)),
            },
            Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => break Err(Failure::new("doubao-error").with("detail", "the connection was closed")),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(e) => break Err(offline(e)),
        }
        if Instant::now() > deadline {
            break Err(offline("no answer to the request"));
        }
    };
    let _ = socket.close(None);
    let _ = socket.flush();
    answered
}

/// The connection of the recording in progress.
struct Connection {
    audio: Sender<Out>,
    /// What it came to: all of the text, or why there is none.
    ended: Receiver<Result<String, Failure>>,
}

/// Speech to text by Doubao. Made anew for each recording (a connection
/// is a recording's), so it holds nothing worth keeping.
pub(crate) struct Doubao {
    config: Config,
    url: String,
    /// Audio not yet sent: less than a packet.
    waiting: Vec<f32>,
    connection: Option<Connection>,
}

impl Doubao {
    pub(crate) fn new(config: Config) -> Doubao {
        Doubao::at(endpoint(), config)
    }

    fn at(url: String, config: Config) -> Doubao {
        Doubao { config, url, waiting: Vec::new(), connection: None }
    }
}

impl Recognizer for Doubao {
    fn begin(&mut self, say: Say) {
        self.waiting.clear();
        let (audio, given) = mpsc::channel();
        let (ending, ended) = mpsc::channel();
        let (url, config) = (self.url.clone(), self.config.clone());
        let failing = ending.clone();
        let started = std::thread::Builder::new().name("dictation-doubao".into()).spawn(move || {
            let mut heard = Heard::new(say);
            let result = session(&url, &config, &given, &mut heard);
            if let Err(failure) = &result {
                if failure.code != "cancelled" {
                    heard.failed(failure);
                }
            }
            let _ = ending.send(result.map(|()| heard.all));
        });
        if let Err(e) = started {
            let _ = failing.send(Err(Failure::failed(e)));
        }
        self.connection = Some(Connection { audio, ended });
    }

    fn feed(&mut self, samples: &[f32]) {
        let Some(connection) = &self.connection else { return };
        self.waiting.extend_from_slice(samples);
        while self.waiting.len() >= PACKET {
            let packet: Vec<f32> = self.waiting.drain(..PACKET).collect();
            let _ = connection.audio.send(Out::Audio(pcm(&packet)));
        }
    }

    fn finish(&mut self) -> Result<String, Failure> {
        let Some(connection) = self.connection.take() else { return Ok(String::new()) };
        let _ = connection.audio.send(Out::Last(pcm(&std::mem::take(&mut self.waiting))));
        match connection.ended.recv_timeout(CONNECTING + LAST_ANSWER) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(offline("no answer after the end of the recording")),
            Err(RecvTimeoutError::Disconnected) => Err(Failure::failed("the connection ended unexpectedly")),
        }
    }
}

// Dropping the connection's sender is what closes it: a recording that is
// discarded is let go without `finish`.

#[cfg(test)]
pub(crate) mod mock;

#[cfg(test)]
mod tests;
