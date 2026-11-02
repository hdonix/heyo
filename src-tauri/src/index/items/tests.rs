use super::*;
use crate::index::embed::Role;

/// Wednesday 8 October 2025, 15:30.
fn now() -> DateTime<Local> {
    Local.with_ymd_and_hms(2025, 10, 8, 15, 30, 0).unwrap()
}

/// `now` and so many days and hours from it, as ISO 8601 with an offset.
fn at(days: i64, hours: i64) -> String {
    let midnight = Local.with_ymd_and_hms(2025, 10, 8, 0, 0, 0).unwrap();
    (midnight + chrono::Duration::days(days) + chrono::Duration::hours(hours)).to_rfc3339()
}

fn person(name: &str, address: &str, role: PersonRole) -> Person {
    Person { name: (!name.is_empty()).then(|| name.to_owned()), address: (!address.is_empty()).then(|| address.to_owned()), role }
}

fn item(id: &str, kind: ItemKind, title: &str, time: String) -> Item {
    Item { id: id.into(), kind, title: title.into(), people: None, time, end: None, text: None, url: None, location: None, attachments: None, folder: None, flags: None }
}

fn mail(id: &str, title: &str, from: Person, time: String, text: &str) -> Item {
    Item { people: Some(vec![from, person("Me", "me@example.com", PersonRole::To)]), text: Some(text.into()), folder: Some("INBOX".into()), ..item(id, ItemKind::Email, title, time) }
}

fn zhang() -> Person {
    person("张三", "zhang.san@example.com", PersonRole::From)
}

fn sample() -> Items {
    let items = Items::in_memory();
    let budget = Item {
        attachments: Some(vec![Attachment { name: "budget.xlsx".into(), size: Some(18_000), mime: None }]),
        flags: Some(vec!["seen".into()]),
        ..mail("m1", "Q3 预算 review", zhang(), at(-1, 14), "请看附件里的预算表 https://example.com/q3/budget")
    };
    let lunch = mail("m2", "Lunch on Friday?", person("Li Si", "li@example.com", PersonRole::From), at(-3, 12), "Shall we try the new noodle place");
    let weekly = Item {
        people: Some(vec![person("王五", "wang@example.com", PersonRole::Organizer), person("张三", "zhang.san@example.com", PersonRole::Attendee)]),
        end: Some(at(1, 16)),
        location: Some("3楼会议室".into()),
        ..item("e1", ItemKind::Event, "周会 Wayland 支持", at(1, 15))
    };
    let review = Item { end: Some(at(-8, 11)), ..item("e2", ItemKind::Event, "Design review", at(-8, 10)) };
    let manual = Item { text: Some("ssh deploy@10.0.3.17 -p 2222".into()), url: Some("https://docs.example.com/deploy".into()), ..item("d1", ItemKind::Document, "部署手册", at(-10, 9)) };
    let report = item("d2", ItemKind::Document, "昨天的报告写完了", at(-5, 9));
    let passport = item("t1", ItemKind::Task, "Renew passport", "2025-10-20".into());
    items.put("work", &[budget, lunch, weekly, review, manual, report, passport]).unwrap();
    items.put("home", &[mail("h1", "发票已开", zhang(), at(0, 9), "发票请查收")]).unwrap();
    items
}

fn ids(items: &Items, query: &str) -> Vec<String> {
    items.search(query, 50, now()).unwrap().into_iter().map(|found| found.item.id).collect()
}

#[test]
fn items_are_put_replaced_and_taken_away() {
    let items = sample();
    assert_eq!((items.count("work").unwrap(), items.count("home").unwrap(), items.count("nobody").unwrap()), (7, 1, 0));
    // The same id again replaces it, words and all.
    items.put("work", &[mail("m2", "Dinner on Saturday?", person("Li Si", "li@example.com", PersonRole::From), at(-2, 12), "ramen instead")]).unwrap();
    assert_eq!(items.count("work").unwrap(), 7);
    assert!(ids(&items, "noodle").is_empty());
    assert_eq!(ids(&items, "ramen"), ["m2"]);
    // The same id in another account is another item.
    items.put("home", &[item("m2", ItemKind::Task, "Buy ramen", at(0, 8))]).unwrap();
    assert_eq!(items.search("ramen", 10, now()).unwrap().iter().map(|found| found.account.as_str()).collect::<Vec<_>>(), ["home", "work"]);

    items.remove("work", &["m2".into(), "no such".into()]).unwrap();
    assert_eq!(items.count("work").unwrap(), 6);
    assert_eq!(ids(&items, "ramen"), ["m2"]);
    // Over before the 1st: the review (30 Sep), the manual (28 Sep).
    assert_eq!(items.prune("work", Local.with_ymd_and_hms(2025, 10, 1, 0, 0, 0).unwrap().with_timezone(&Utc)).unwrap(), 2);
    assert_eq!(items.count("work").unwrap(), 4);
    assert!(ids(&items, "0.3.17").is_empty());
    items.drop_account("home").unwrap();
    assert_eq!(items.count("home").unwrap(), 0);
    assert_eq!(ids(&items, "发票"), Vec::<String>::new());
    assert_eq!(items.count("work").unwrap(), 4);
    // Nothing is left of what went: no words, people or vectors.
    let db = items.db.lock().unwrap();
    let left: i64 = db.query_row("SELECT count(*) FROM people WHERE item NOT IN (SELECT rowid FROM items)", [], |row| row.get(0)).unwrap();
    assert_eq!(left, 0);
}

#[test]
fn what_is_stored_is_what_was_put() {
    let items = sample();
    let found = items.search("budget.xlsx", 10, now()).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].account, "work");
    let budget = &found[0].item;
    assert_eq!(budget.attachments.as_ref().unwrap()[0], Attachment { name: "budget.xlsx".into(), size: Some(18_000), mime: None });
    assert_eq!(budget.people.as_ref().unwrap()[0], zhang());
    assert_eq!((budget.time.as_str(), budget.flags.as_deref()), (at(-1, 14).as_str(), Some(&["seen".to_owned()][..])));
    assert!(found[0].score > 0.0);
    // The time as it was given: a date alone stays a date.
    assert_eq!(items.search("passport", 10, now()).unwrap()[0].item.time, "2025-10-20");
    // On the wire: camelCase, and nothing for what is not there.
    let wire = serde_json::to_value(&found[0]).unwrap();
    assert_eq!(wire["item"]["attachments"][0], serde_json::json!({ "name": "budget.xlsx", "size": 18000 }));
    assert!(wire["item"].get("end").is_none() && wire["account"] == "work");

    // A long text is cut.
    let long = Item { text: Some("字".repeat(MAX_TEXT + 100)), ..item("long", ItemKind::Document, "A long one", at(0, 1)) };
    items.put("work", &[long]).unwrap();
    assert_eq!(items.search("long one", 1, now()).unwrap()[0].item.text.as_ref().unwrap().chars().count(), MAX_TEXT);
    // A time that is not one stops the whole put.
    let bad = [item("fine", ItemKind::Task, "Fine", at(0, 1)), item("bad", ItemKind::Task, "Bad", "next Tuesday".into())];
    assert!(items.put("work", &bad).is_err());
    assert!(ids(&items, "fine").is_empty());
}

#[test]
fn words_pieces_and_chinese() {
    let items = sample();
    assert_eq!(ids(&items, "预算"), ["m1"]);
    assert_eq!(ids(&items, "算"), ["m1"]);
    assert_eq!(ids(&items, "noodle"), ["m2"]);
    assert_eq!(ids(&items, "wayland 支持"), ["e1"]);
    // Pieces of addresses, numbers and file names, through the trigrams.
    assert_eq!(ids(&items, "q3/budget"), ["m1"]);
    assert_eq!(ids(&items, "0.3.17"), ["d1"]);
    assert_eq!(ids(&items, "XLSX"), ["m1"]);
    // Where it is, and who: words too.
    assert_eq!(ids(&items, "会议室"), ["e1"]);
    assert_eq!(ids(&items, "li si"), ["m2"]);
    for odd in ["\"", "a\"b", "NOT", "(", "*", "col:on", "-p", "=>", "from:", "发的"] {
        items.search(odd, 10, now()).unwrap();
    }
    // Nothing asked: everything, nearest to now first.
    assert_eq!(ids(&items, "")[..3], ["h1", "e1", "m1"]);
}

#[test]
fn times_ahead_and_behind() {
    let items = sample();
    assert_eq!(ids(&items, "昨天"), ["m1"]);
    assert_eq!(ids(&items, "yesterday afternoon"), ["m1"]);
    assert_eq!(ids(&items, "明天"), ["e1"]);
    assert_eq!(ids(&items, "上周二"), ["e2"]);
    // A meeting that runs into the hour is in it.
    assert_eq!(ids(&items, "明天下午"), ["e1"]);
    assert_eq!(ids(&items, "next month"), Vec::<String>::new());
    assert_eq!(ids(&items, "10月20日"), ["t1"]);
    // "昨天" reads as a time, but no report is of yesterday: the words win.
    assert_eq!(ids(&items, "昨天的报告"), ["d2"]);
}

#[test]
fn people_and_kinds() {
    let items = sample();
    assert_eq!(ids(&items, "from:张三"), ["h1", "m1"]);
    assert_eq!(ids(&items, "张三发的"), ["h1", "m1"]);
    assert_eq!(ids(&items, "张三发的邮件 预算"), ["m1"]);
    assert_eq!(ids(&items, "from zhang"), ["h1", "m1"]);
    assert_eq!(ids(&items, "from li@"), ["m2"]);
    assert_eq!(ids(&items, "和张三的会议"), ["e1"]);
    assert_eq!(ids(&items, "from:王五"), ["e1"]);
    assert_eq!(ids(&items, "邮件"), ["h1", "m1", "m2"]);
    assert_eq!(ids(&items, "mail"), ["h1", "m1", "m2"]);
    assert_eq!(ids(&items, "日程"), ["e1", "e2"]);
    assert_eq!(ids(&items, "明天的会议"), ["e1"]);
    assert_eq!(ids(&items, "tasks"), ["t1"]);
    assert_eq!(ids(&items, "有附件的邮件"), ["m1"]);
    assert_eq!(ids(&items, "with attachments"), ["m1"]);
    // Nobody of that name: words, which find nothing.
    assert!(ids(&items, "letter from grandma").is_empty());
}

/// Gives a text a vector along one axis, by its subject, and asks nothing
/// of a helper.
struct Fake;

impl Embed for Fake {
    fn model(&self) -> &str {
        "fake"
    }

    fn embed(&self, _role: Role, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let subjects: [&[&str]; 3] = [&["预算", "budget", "spending"], &["noodle", "lunch", "food"], &["passport", "visa"]];
        Ok(texts
            .iter()
            .map(|text| {
                let text = text.to_lowercase();
                let axis = subjects.iter().position(|words| words.iter().any(|word| text.contains(word))).unwrap_or(3 + text.chars().count() % 13);
                let mut vector = vec![0f32; 16];
                vector[axis] = 1.0;
                vector
            })
            .collect())
    }
}

fn with_meaning(items: Items) -> Arc<Items> {
    let items = Arc::new(items);
    let _ = items.meaning.set(Meaning { model: Arc::new(Fake), enabled: Box::new(|| true) });
    items
}

#[test]
fn the_meaning_finds_what_the_words_do_not() {
    let items = with_meaning(sample());
    items.put("work", &[item("d3", ItemKind::Document, "预算说明", at(-20, 9))]).unwrap();
    assert_eq!(items.vector_counts("fake").unwrap(), (0, 9));
    while meaning::step(&*items, &Fake, 3, |_, _, _| {}).unwrap() > 0 {}
    assert_eq!(items.vector_counts("fake").unwrap(), (9, 0));
    let vector = |text: &str| Fake.embed(Role::Query, &[text.to_owned()]).unwrap().remove(0);
    let found = |query: &str| items.search_with(query, 10, now(), Some(&vector(query))).unwrap();
    // No word of it in the mail; its meaning finds it, first.
    let spending = found("spending plan");
    assert_eq!(spending[0].item.id, "m1");
    assert!(ids(&items, "spending plan").is_empty());
    // The words and the meaning agree: once.
    let lunch = found("lunch");
    assert_eq!(lunch.iter().filter(|found| found.item.id == "m2").count(), 1);
    assert_eq!(lunch[0].item.id, "m2");
    assert_eq!(spending.iter().take(2).map(|found| found.item.id.as_str()).collect::<Vec<_>>(), ["m1", "d3"]);
    // A kind holds for the meaning too.
    assert_eq!(found("文档 spending plan")[0].item.id, "d3");
    assert!(found("文档 spending plan").iter().all(|found| found.item.kind == ItemKind::Document));
    // Changed words, a new vector; a deleted item is not found by its old one.
    items.put("work", &[mail("m2", "Holiday plans", person("Li Si", "li@example.com", PersonRole::From), at(-3, 12), "Going to the beach")]).unwrap();
    assert_eq!(items.vector_counts("fake").unwrap(), (8, 1));
    items.drop_account("work").unwrap();
    assert!(found("spending plan").is_empty());
}

#[test]
fn vectors_are_made_in_the_background() {
    let items = Arc::new(Items::in_memory());
    items.start_meaning(Arc::new(Fake), || true);
    let start = Instant::now();
    items.put("work", &[mail("m1", "Q3 预算 review", zhang(), at(-1, 14), "the numbers")]).unwrap();
    // The put does not wait for them.
    assert!(start.elapsed() < Duration::from_millis(500));
    while items.vector_counts("fake").unwrap() != (1, 0) {
        assert!(start.elapsed() < Duration::from_secs(10), "no vectors");
        std::thread::sleep(Duration::from_millis(20));
    }
    let found = items.search_waiting("spending plan", 10, now(), Duration::from_secs(10)).unwrap();
    assert_eq!(found.iter().map(|found| found.item.id.as_str()).collect::<Vec<_>>(), ["m1"]);
}

#[cfg(unix)]
#[test]
fn the_file_is_the_users_alone() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("heyo-items-{}", std::process::id()));
    let path = dir.join("connections.db");
    assert!(Items::path().ends_with("connections.db"));
    let items = Items::open(&path).unwrap();
    items.put("work", &[item("t", ItemKind::Task, "Something", at(0, 0))]).unwrap();
    for suffix in ["", "-wal"] {
        let mode = std::fs::metadata(dir.join(format!("connections.db{suffix}"))).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "connections.db{suffix}");
    }
    drop(items);
    assert_eq!(Items::open(&path).unwrap().count("work").unwrap(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

/// `cargo test --release index::items::tests::speed -- --ignored --nocapture`
#[test]
#[ignore]
fn speed() {
    let items = Items::in_memory();
    let subjects = ["发票", "合同", "会议纪要", "服务器地址", "password reset", "invoice", "deploy script", "接口文档", "报错日志", "quarterly budget"];
    let names = [("张三", "zhang.san"), ("李四", "li.si"), ("王五", "wang.wu"), ("Alice Chen", "alice"), ("Bob Lee", "bob")];
    let all: Vec<Item> = (0..10_000i64)
        .map(|i| {
            let (name, address) = names[(i % 5) as usize];
            let kind = [ItemKind::Email, ItemKind::Email, ItemKind::Email, ItemKind::Event, ItemKind::Document][(i % 5) as usize];
            let text = format!("{} 第{i}封，关于{}。 https://example.com/item/{i}?ref={} {}", subjects[(i * 7 % 10) as usize], subjects[(i % 10) as usize], i * 7919 % 1000, "正文 body text ".repeat(20));
            Item {
                people: Some(vec![person(name, &format!("{address}@example.com"), PersonRole::From)]),
                text: Some(text),
                attachments: (i % 4 == 0).then(|| vec![Attachment { name: format!("file{i}.pdf"), size: None, mime: None }]),
                ..item(&format!("i{i}"), kind, &format!("{} #{i}", subjects[(i % 10) as usize]), at(-(i / 40), i % 24))
            }
        })
        .collect();
    let start = Instant::now();
    for chunk in all.chunks(500) {
        items.put("work", chunk).unwrap();
    }
    println!("10000 items put in {:.0} ms", start.elapsed().as_secs_f64() * 1000.0);
    let mut worst: f64 = 0.0;
    for query in ["发票", "票", "会议纪要 报错", "item/4242", "ref=77", "de", "昨天 合同", "from:张三 invoice", "张三发的邮件", "上周的会议", "有附件的 budget", "quarterly budget", "没有这个词"] {
        let mut times: Vec<f64> = (0..50)
            .map(|_| {
                let start = Instant::now();
                items.search(query, 50, now()).unwrap();
                start.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        times.sort_by(f64::total_cmp);
        worst = worst.max(times[47]);
        println!("{query:<24} p50 {:6.2} ms   p95 {:6.2} ms   ({} found)", times[25], times[47], items.search(query, 50, now()).unwrap().len());
    }
    assert!(worst < 50.0, "the slowest query took {worst:.1} ms");
}
