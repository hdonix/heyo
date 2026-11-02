use std::sync::mpsc;
use std::time::Duration;

use serde_json::json;

use super::mock::{self, Behaviour};
use super::*;
use crate::dictation::vocabulary;

fn config(key: Key) -> Config {
    Config { key, resource: "volc.bigasr.sauc.duration".into(), remove_fillers: true, hotwords: vec!["TypeScript".into(), "杰森".into()] }
}

/// Starts a recording against `url`: what it says comes to the receiver.
fn begun(url: String, config: Config) -> (Doubao, mpsc::Receiver<Recognized>) {
    let (tx, rx) = mpsc::channel();
    let mut doubao = Doubao::at(url, config);
    doubao.begin(Box::new(move |recognized| drop(tx.send(recognized))));
    (doubao, rx)
}

/// Feeds `seconds` of a tone, a tenth of a second at a time.
fn speak(doubao: &mut Doubao, seconds: f32) {
    let tenth: Vec<f32> = (0..SAMPLE_RATE / 10).map(|at| (at as f32 / 8.0).sin() * 0.3).collect();
    for _ in 0..(seconds * 10.0).round() as usize {
        doubao.feed(&tenth);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn packets_are_framed_as_the_documentation_says() {
    let request = packet(FULL_CLIENT_REQUEST, 0, JSON, br#"{"a":1}"#);
    // Version 1, header of 1 × 4 bytes; full client request, no flags;
    // JSON, gzip; reserved.
    assert_eq!(request[..4], [0x11, 0x10, 0x11, 0x00]);
    let size = u32::from_be_bytes(request[4..8].try_into().unwrap()) as usize;
    assert_eq!(size, request.len() - 8);
    assert_eq!(gunzip(&request[8..]).unwrap(), br#"{"a":1}"#);
    // Audio: no serialization, gzip; the last one flagged 0b0010.
    assert_eq!(packet(AUDIO_ONLY_REQUEST, 0, RAW, &[0; 6400])[..4], [0x11, 0x20, 0x01, 0x00]);
    assert_eq!(packet(AUDIO_ONLY_REQUEST, LAST, RAW, &[])[..4], [0x11, 0x22, 0x01, 0x00]);
    // 16-bit little-endian samples.
    assert_eq!(pcm(&[0.0, 1.0, -1.0, 2.0]), [0, 0, 0xff, 0x7f, 0x01, 0x80, 0xff, 0x7f]);
}

#[test]
fn answers_are_read_as_the_documentation_says() {
    let result = json!({ "result": { "text": "你好" } });
    assert_eq!(answer(&mock::response(3, false, &result)).unwrap(), Answer::Heard { result: result.clone(), last: false });
    assert_eq!(answer(&mock::response(4, true, &result)).unwrap(), Answer::Heard { result, last: true });
    assert_eq!(answer(&mock::error(45000001, "invalid request")).unwrap(), Answer::Error { code: 45000001, message: "invalid request".into() });
    // A header that says it is longer: its extensions are skipped.
    let mut longer = mock::response(1, false, &json!({}));
    longer[0] = 0x12;
    longer.splice(4..4, [0xaa; 4]);
    assert!(matches!(answer(&longer).unwrap(), Answer::Heard { last: false, .. }));
    assert_eq!(answer(&[0x11, 0x90]).unwrap_err().code, "doubao-error");
    assert_eq!(answer(&[0x11, 0x30, 0x11, 0, 0, 0, 0, 0]).unwrap_err().code, "doubao-error");
}

#[test]
fn a_recording_is_sent_and_heard_sentence_by_sentence() {
    let sentences = vec!["Hello there.".to_owned(), "This is Heyo.".to_owned()];
    let (url, served) = mock::once(Behaviour::Hears { sentences, every: 3 });
    let (mut doubao, heard) = begun(url, config(Key::Api("the-key".into())));
    speak(&mut doubao, 1.3);
    assert_eq!(doubao.finish().unwrap(), "Hello there. This is Heyo.");
    let heard: Vec<Recognized> = heard.try_iter().collect();
    let sentences: Vec<&Recognized> = heard.iter().filter(|said| matches!(said, Recognized::Sentence(_))).collect();
    assert_eq!(sentences, [&Recognized::Sentence("Hello there.".into()), &Recognized::Sentence(" This is Heyo.".into())]);
    // The one in progress before each, set apart from what came before.
    assert_eq!(heard[0], Recognized::Partial("Hello t".into()));
    assert!(heard.contains(&Recognized::Partial(" This is".into())), "{heard:?}");

    let seen = served.join().unwrap();
    assert!(seen.wrong.is_empty(), "{:?}", seen.wrong);
    assert_eq!(seen.path, "/api/v3/sauc/bigmodel_async");
    assert_eq!(seen.headers["x-api-key"], "the-key");
    assert_eq!(seen.headers["x-api-resource-id"], "volc.bigasr.sauc.duration");
    assert!(!seen.headers.contains_key("x-api-app-key") && !seen.headers.contains_key("x-api-access-key"));
    let id = &seen.headers["x-api-connect-id"];
    assert!(id.len() == 36 && id.as_bytes()[14] == b'4' && id.split('-').map(str::len).eq([8, 4, 4, 4, 12]), "{id}");
    assert_eq!(seen.request["audio"], json!({ "format": "pcm", "codec": "raw", "rate": 16000, "bits": 16, "channel": 1 }));
    let request = &seen.request["request"];
    assert_eq!((request["model_name"].as_str(), request["enable_itn"].as_bool(), request["enable_punc"].as_bool(), request["enable_ddc"].as_bool()), (Some("bigmodel"), Some(true), Some(true), Some(true)));
    assert_eq!((request["show_utterances"].as_bool(), request["result_type"].as_str(), request["enable_nonstream"].as_bool()), (Some(true), Some("full"), Some(true)));
    let context: serde_json::Value = serde_json::from_str(request["corpus"]["context"].as_str().unwrap()).unwrap();
    assert_eq!(context, json!({ "hotwords": [{ "word": "TypeScript" }, { "word": "杰森" }] }));
    // 1.3 s: six packets of 200 ms, 6400 bytes each, and the last 100 ms
    // in the last one, which says it is.
    assert_eq!(seen.packets, [6400, 6400, 6400, 6400, 6400, 6400, 3200]);
    assert!(seen.last && seen.closed);
}

#[test]
fn the_old_consoles_key_is_an_app_id_and_access_token() {
    let (url, served) = mock::once(Behaviour::Hears { sentences: vec!["好。".into()], every: 1 });
    let (mut doubao, _heard) = begun(url, config(Key::App { id: "123456789".into(), token: "the-token".into() }));
    speak(&mut doubao, 0.2);
    assert_eq!(doubao.finish().unwrap(), "好。");
    let seen = served.join().unwrap();
    assert_eq!((seen.headers["x-api-app-key"].as_str(), seen.headers["x-api-access-key"].as_str()), ("123456789", "the-token"));
    assert!(!seen.headers.contains_key("x-api-key"));
}

#[test]
fn a_key_that_is_refused_is_said_at_once_as_such() {
    for (status, code) in [(401, "doubao-key"), (403, "doubao-denied"), (429, "doubao-busy"), (500, "doubao-error")] {
        let (url, served) = mock::once(Behaviour::Refuses(status, "{\"error\":\"nope\"}"));
        let (mut doubao, heard) = begun(url, config(Key::Api("wrong".into())));
        // Said while the recording goes on, which it then ends.
        let Ok(Recognized::Failed(failure)) = heard.recv_timeout(Duration::from_secs(5)) else { panic!("no failure") };
        assert_eq!(failure.code, code);
        assert_eq!(failure.particulars, [("detail", format!("HTTP {status} {{\"error\":\"nope\"}}"))]);
        speak(&mut doubao, 0.2);
        assert_eq!(doubao.finish().unwrap_err(), failure);
        served.join().unwrap();
    }
    let worded = crate::helper::worded(&Failure::new("doubao-key").with("detail", "HTTP 401").said(), "Dictation failed");
    assert_eq!(worded.short, "Doubao did not accept the key");
}

#[test]
fn what_was_being_said_when_it_failed_is_kept() {
    let (url, served) = mock::once(Behaviour::Fails { after: 3, code: 55000031, message: "server busy" });
    let (mut doubao, heard) = begun(url, config(Key::Api("the-key".into())));
    speak(&mut doubao, 1.0);
    let failure = Failure::new("doubao-busy").with("detail", "55000031: server busy");
    assert_eq!(doubao.finish().unwrap_err(), failure);
    let heard: Vec<Recognized> = heard.try_iter().collect();
    assert_eq!(heard, [Recognized::Partial("你好世".into()), Recognized::Sentence("你好世".into()), Recognized::Failed(failure)]);
    assert!(served.join().unwrap().wrong.is_empty());
}

#[test]
fn a_recording_that_is_discarded_closes_the_connection() {
    let (url, served) = mock::once(Behaviour::Hears { sentences: vec!["好。".into()], every: 100 });
    let (mut doubao, heard) = begun(url, config(Key::Api("the-key".into())));
    speak(&mut doubao, 0.6);
    drop(doubao);
    let seen = served.join().unwrap();
    assert!(seen.closed && !seen.last, "{seen:?}");
    assert!(!seen.packets.is_empty());
    // Nothing is said of it failing: it did not.
    assert!(heard.try_iter().all(|said| !matches!(said, Recognized::Failed(_))));
}

#[test]
fn a_service_that_cannot_be_reached_is_said_to_be() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let (mut doubao, heard) = begun(format!("ws://127.0.0.1:{port}/"), config(Key::Api("k".into())));
    assert!(matches!(heard.recv_timeout(Duration::from_secs(5)), Ok(Recognized::Failed(Failure { code: "doubao-unreachable", .. }))));
    assert_eq!(doubao.finish().unwrap_err().code, "doubao-unreachable");
}

#[test]
fn the_providers_settings_make_a_config_with_the_key_of_its_console_or_none() {
    use crate::settings::Settings;
    let settings = Settings {
        doubao_api_key: " k ".into(),
        doubao_app_id: " id ".into(),
        doubao_access_token: "token".into(),
        doubao_remove_fillers: true,
        dictation_vocabulary: vec!["TypeScript".into(), "杰森 -> JSON".into()],
        ..Settings::default()
    };
    // (Keys are compared, never printed.)
    let made = Config::from_settings(&settings, RESOURCES[2].0).unwrap();
    assert!(made.key == Key::Api("k".into()));
    assert_eq!((made.resource.as_str(), made.remove_fillers, made.hotwords), (RESOURCES[2].0, true, vec!["TypeScript".to_owned(), "JSON".to_owned()]));
    let old = Settings { doubao_console: "old".into(), ..settings.clone() };
    assert!(Config::from_settings(&old, "").unwrap().key == Key::App { id: "id".into(), token: "token".into() });
    // No model: the first; one it does not have: none.
    assert_eq!(Config::from_settings(&old, "").unwrap().resource, RESOURCES[0].0);
    assert_eq!(Config::from_settings(&old, "volc.other").unwrap_err().code, "failed");
    // The console's key is missing.
    for keyless in [Settings { doubao_api_key: "".into(), ..settings.clone() }, Settings { doubao_access_token: " ".into(), ..old.clone() }, Settings { doubao_app_id: "".into(), ..old.clone() }] {
        assert_eq!(Config::from_settings(&keyless, "").unwrap_err(), Failure::new("doubao-no-key"));
    }
    assert!(recognizer(&settings, "").is_ok());
    // The key is never printed.
    let printed = format!("{:?}", Config::from_settings(&Settings { doubao_api_key: "sk-secret".into(), ..settings }, "").unwrap());
    assert!(!printed.contains("sk-secret") && printed.contains("API key"), "{printed}");
}

#[test]
fn the_provider_says_what_it_is_and_its_fields_are_settings() {
    let provider = provider();
    assert_eq!(provider.id, "doubao");
    assert_eq!(provider.capabilities, [Capability::StreamingSpeechToText, Capability::Cloud]);
    assert_eq!(provider.models.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ["volc.seedasr.sauc.duration", "volc.seedasr.sauc.concurrent", "volc.bigasr.sauc.duration", "volc.bigasr.sauc.concurrent"]);
    let settings = serde_json::to_value(crate::settings::Settings::default()).unwrap();
    for field in provider.fields {
        assert!(settings.get(field.setting).is_some(), "{} is no setting", field.setting);
        assert_eq!(field.kind == FieldKind::Secret, crate::settings::SECRETS.contains(&field.setting), "{}", field.setting);
        if let Some((other, value)) = field.shown_when {
            assert!(settings.get(other).is_some(), "{other}");
            assert!(provider.fields.iter().any(|field| field.setting == other && matches!(field.kind, FieldKind::Choice(choices) if choices.iter().any(|(choice, _)| *choice == value))));
        }
    }
}

#[test]
fn a_check_connects_asks_and_hears_the_first_answer() {
    let (url, served) = mock::once(Behaviour::Hears { sentences: vec![], every: 1 });
    assert_eq!(check_at(&url, &config(Key::Api("the-key".into()))), Ok(()));
    let seen = served.join().unwrap();
    assert!(seen.wrong.is_empty() && seen.packets.is_empty() && seen.closed, "{seen:?}");
    assert_eq!(seen.request["request"]["model_name"], "bigmodel");
    let (url, served) = mock::once(Behaviour::Refuses(401, "unauthorized"));
    assert_eq!(check_at(&url, &config(Key::Api("wrong".into()))).unwrap_err().code, "doubao-key");
    served.join().unwrap();
}

#[test]
fn hotwords_are_as_many_as_fit_in_100_tokens() {
    let lines: Vec<String> = (0..60).map(|at| format!("Word{at} Two")).chain(["TypeScript".into()]).collect();
    let words = hotwords(&vocabulary::parse(&lines));
    assert_eq!(words.len(), 50);
    // A Chinese word costs a token a character.
    let lines: Vec<String> = ["豆包语音识别".repeat(16), "杰森".into(), "大模型".into()].into();
    assert_eq!(hotwords(&vocabulary::parse(&lines)), ["豆包语音识别".repeat(16), "杰森".into()]);
}
