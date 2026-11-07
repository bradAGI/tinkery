//! The prompts reach Pi byte for byte as recorded. The recorded copies in
//! `tests/fixtures/prompts/` were taken from main before the prompts moved into their own files
//! (#38); a wording change must update them on purpose, with an eval behind it.
use super::*;

/// The system prompts Pi receives for one send: the reading, then the decision review.
#[cfg(unix)]
fn sent_prompts(words: &str) -> (String, String) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("pi");
    std::fs::write(&program, r#"#!/usr/bin/env python3
import json,sys,pathlib
root=pathlib.Path(__file__).parent
r=json.load(sys.stdin)
kind=r.get('kind','shape')
(root/('request-'+kind)).write_text(json.dumps(r))
p=sys.argv[sys.argv.index('--system-prompt')+1]
(root/('prompt-'+kind)).write_bytes(p.encode('utf-8'))
if kind=='decision-review':print(json.dumps(dict(decisions=[])));sys.exit(0)
print(json.dumps(dict(parts=dict(who='I',outcome='judge the result',why='it matters',done_when='a judged result exists'),questions=[])))
"#).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let pi = ModelHost::new(
        crate::pi::Harness::Pi,
        program,
        "test/model".into(),
        crate::pi::CallBudget::new(2),
    )
    .unwrap();
    let request = BoardRequest {
        ask_counts: Default::default(),
        sources: vec![Source {
            id: 1,
            text: words.into(),
            in_reply_to: None,
        }],
        fragments: vec![],
        previous: None,
        skipped: vec![],
        answered: vec![],
        settled: vec![],
        layout: vec![],
    };
    pi.reshape(request, &AtomicBool::new(false)).unwrap();
    let sent: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("request-shape")).unwrap())
            .unwrap();
    assert_eq!(sent["sources"][0]["text"], words);
    let read =
        |kind: &str| std::fs::read_to_string(dir.path().join(format!("prompt-{kind}"))).unwrap();
    (read("shape"), read("decision-review"))
}

fn recorded(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/prompts")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

#[cfg(unix)]
#[test]
fn prompts_reach_pi_byte_for_byte_as_recorded() {
    let (reading, review) = sent_prompts("the restaurateur judges the result");
    assert_eq!(reading, recorded("reading.txt"));
    assert_eq!(review, recorded("decision_review.txt"));
}

/// Pins the sent rule, not the model's compliance: that requires the separately budgeted live eval.
#[cfg(unix)]
#[test]
fn slice_exclusion_rule_reaches_pi_with_the_recorded_169_dump() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("exclusion_replay.json")).unwrap();
    let (prompt, _) = sent_prompts(fixture["words"].as_str().unwrap());
    for rule in [
        "Explicit exclusions such as \"do not fix X in this slice\" belong in must_not with their scope qualifier intact, never as positive work in deferred or other_goals.",
        "Deferred items require an affirmative request to do the work later; being excluded now is not that request.",
        "If the person both excludes work now and asks for it later, retain the current-slice must_not and the separately scoped deferred request.",
    ] {
        assert!(
            prompt.contains(rule),
            "Missing sent exclusion/deferral rule: {rule}"
        );
    }
}

/// Replay of eval2-think/decoder-probe-v2/array_original.json (#202), through a stub Pi.
#[cfg(unix)]
#[test]
fn recorded_done_when_list_survives_stub_replay() {
    use std::os::unix::fs::PermissionsExt;
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("done_when_replay.json")).unwrap();
    let raw = fixture["replies"][0]["text"].as_str().unwrap();
    let recorded: serde_json::Value = serde_json::from_str(raw).unwrap();
    let checks: Vec<_> = recorded["parts"]["done_when"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("pi");
    std::fs::write(dir.path().join("reply"), raw).unwrap();
    std::fs::write(
        &program,
        r#"#!/usr/bin/env python3
import json,sys,pathlib
r=json.load(sys.stdin)
if r.get('kind')=='decision-review':print('{"decisions":[]}')
else:print(pathlib.Path(__file__).with_name('reply').read_text())
"#,
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let pi = ModelHost::new(
        crate::pi::Harness::Pi,
        program,
        "test/model".into(),
        crate::pi::CallBudget::new(4),
    )
    .unwrap();
    let words = fixture["words"].as_str().unwrap();
    let request = BoardRequest {
        ask_counts: Default::default(),
        sources: vec![Source {
            id: 1,
            text: words.into(),
            in_reply_to: None,
        }],
        fragments: vec![],
        previous: None,
        skipped: vec![],
        answered: vec![],
        settled: vec![],
        layout: vec![],
    };
    for done_when in [
        recorded["parts"]["done_when"].clone(),
        checks.join("\n").into(),
    ] {
        let mut reply = recorded.clone();
        reply["parts"]["done_when"] = done_when;
        let sent = if reply == recorded {
            raw.to_owned()
        } else {
            reply.to_string()
        };
        std::fs::write(dir.path().join("reply"), &sent).unwrap();
        let guess = pi
            .reshape(request.clone(), &AtomicBool::new(false))
            .unwrap();
        assert_eq!(
            guess.parts.done_when.as_deref(),
            Some(checks.join("\n").as_str())
        );
        assert!(guess.questions.is_empty(), "{:?}", guess.questions);
        let board = board::Board::verify(guess, &request).unwrap();
        let reading = &board.reading.as_ref().unwrap().text;
        let mut text_reply = recorded.clone();
        text_reply["parts"]["done_when"] = checks.join("\n").into();
        let expected = Guess::decode(&text_reply.to_string(), &request, &mut vec![]).unwrap();
        let expected = board::Board::verify(expected, &request).unwrap();
        assert_eq!(reading, &expected.reading.as_ref().unwrap().text);
        assert!(board.questions.is_empty());
        assert_eq!(request.sources[0].text, words);
        let record: checks::Decision =
            serde_json::from_str(pi.diagnostics().last().unwrap()).unwrap();
        assert_eq!(
            record.raw.as_deref().map(str::trim_end),
            Some(sent.trim_end())
        );
    }
}

/// Fields the decoder still accepts from older replies but the prompt no longer asks for.
const LEGACY: &[&str] = &["uncertain", "framings", "outcome"];

fn sorted<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut names: Vec<_> = names.into_iter().collect();
    names.sort_unstable();
    names
}

/// The prompt's schema line, split into its top-level entries at commas outside brackets.
fn schema_entries(line: &str) -> Vec<&str> {
    let mut entries = vec![];
    let (mut depth, mut start) = (0, 0);
    for (i, c) in line.char_indices() {
        match c {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                entries.push(line[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    entries.push(line[start..].trim().trim_end_matches('.'));
    entries
}

/// The names inside the first `{...}` of an entry, without "optional".
fn braced(entry: &str) -> Vec<&str> {
    let inner = &entry[entry.find('{').unwrap() + 1..entry.find('}').unwrap()];
    sorted(
        inner
            .split(',')
            .map(|f| f.trim().trim_start_matches("optional ")),
    )
}

#[test]
fn the_reading_prompt_names_the_fields_its_decoder_reads() {
    let line = reading::PROMPT
        .lines()
        .find_map(|l| l.strip_prefix("Return JSON: "))
        .expect("the reading prompt has a schema line");
    let mut top = vec![];
    for entry in schema_entries(line) {
        let name = entry
            .split(['(', '{'])
            .next()
            .unwrap()
            .trim()
            .trim_start_matches("optional ");
        match name {
            "parts" => assert_eq!(braced(entry), sorted(reading::PARTS.iter().copied())),
            "decisions" => assert_eq!(braced(entry), sorted(reading::DECISION.iter().copied())),
            "questions" => assert_eq!(braced(entry), sorted(reading::QUESTION.iter().copied())),
            "alternatives" => assert_eq!(braced(entry), sorted(reading::APPROACH.iter().copied())),
            "supports" => assert!(entry.contains(&reading::ANCHOR.join("/")), "{entry}"),
            _ => {}
        }
        top.extend(name.split(" or "));
    }
    let decoded = reading::READING
        .iter()
        .copied()
        .filter(|f| !LEGACY.contains(f));
    assert_eq!(sorted(top), sorted(decoded));
    // The span rule names the anchor fields the decoder reads.
    assert!(reading::PROMPT.contains(&format!("use {}:", reading::ANCHOR.join("/"))));
}

#[test]
fn the_review_prompt_names_the_fields_its_merge_reads() {
    let example = review::PROMPT
        .lines()
        .find_map(|l| l.strip_prefix("Return ONLY JSON "))
        .and_then(|l| l.rsplit_once("]}").map(|(json, _)| format!("{json}]}}")))
        .expect("the review prompt has a JSON example");
    let example: serde_json::Value = serde_json::from_str(&example).unwrap();
    let object = example.as_object().unwrap();
    assert_eq!(sorted(object.keys().map(String::as_str)), ["decisions"]);
    let decision = object["decisions"][0].as_object().unwrap();
    let mut expected = reading::DECISION.to_vec();
    expected.push(review::COVERS);
    assert_eq!(
        sorted(decision.keys().map(String::as_str)),
        sorted(expected)
    );
    // Every part the review may name is a part think knows.
    let parts = review::PROMPT
        .lines()
        .find_map(|l| l.strip_prefix("part is one of "))
        .unwrap();
    for part in parts.trim_end_matches('.').split(", ") {
        serde_json::from_value::<Part>(part.into()).unwrap_or_else(|_| panic!("{part}"));
    }
}

#[test]
fn notes_never_reach_the_model() {
    for file in [reading::PROMPT, review::PROMPT] {
        let sent = reading::assemble(file);
        assert!(file.lines().any(|l| l.starts_with('#')));
        assert!(!sent.lines().any(|l| l.starts_with('#')), "{sent}");
    }
}

/// The check record keeps the shaping reply exactly as received; the review's decisions reach
/// the reading through its decoder, never by rewriting that reply (#89).
#[cfg(unix)]
#[test]
fn the_recorded_reply_is_the_models_and_the_review_still_joins() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("pi");
    std::fs::write(&program, r#"#!/usr/bin/env python3
import json,sys
r=json.load(sys.stdin)
if r.get('kind')=='decision-review':print(json.dumps(dict(decisions=[dict(part='must_not',text='Whether results may be published',answers=['keep private','may publish'],question='May the results be published?',covers=[])])));sys.exit(0)
print('{"parts":{"who":"I","outcome":"judge the result","why":"it matters","done_when":"a judged result exists"},"questions":[]}')
"#).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let pi = ModelHost::new(
        crate::pi::Harness::Pi,
        program,
        "test/model".into(),
        crate::pi::CallBudget::new(2),
    )
    .unwrap();
    let request = BoardRequest {
        ask_counts: Default::default(),
        sources: vec![Source {
            id: 1,
            text: "the restaurateur judges the result".into(),
            in_reply_to: None,
        }],
        fragments: vec![],
        previous: None,
        skipped: vec![],
        answered: vec![],
        settled: vec![],
        layout: vec![],
    };
    let guess = pi.reshape(request, &AtomicBool::new(false)).unwrap();
    assert_eq!(guess.decisions.len(), 1);
    assert_eq!(guess.questions[0].text, "May the results be published?");
    let record: checks::Decision = serde_json::from_str(pi.diagnostics().last().unwrap()).unwrap();
    assert_eq!(
        record.raw.as_deref().map(str::trim_end),
        Some(
            r#"{"parts":{"who":"I","outcome":"judge the result","why":"it matters","done_when":"a judged result exists"},"questions":[]}"#
        )
    );
    assert!(
        record
            .normalizations
            .contains(&"Decision review added 1 decision(s)".into())
    );
}
