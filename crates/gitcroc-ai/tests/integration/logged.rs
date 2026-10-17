//! AI debug logging (AI-7): the prompt and the reply of each request,
//! retries included, and nothing else.

use std::sync::{Arc, Mutex};

use gitcroc_ai::{FileOutcome, Logged, MockProvider, ask};

use crate::ask::request;
use crate::responses::{HUNK_IDS, VALID};

#[tokio::test]
async fn ai_7_debug_log_has_each_prompt_and_reply() {
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let log = {
        let lines = Arc::clone(&lines);
        Arc::new(move |line: &str| lines.lock().unwrap().push(line.to_owned()))
    };
    let mock = MockProvider::texts(["not json", VALID]);
    let logged = Logged::new(Box::new(mock), log);
    let sent = request();

    let result = ask(&logged, sent.clone(), &HUNK_IDS).await;

    assert!(matches!(result.outcome, FileOutcome::Answered(_)));
    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), 4, "{lines:#?}");
    assert!(lines[0].contains(&sent.system) && lines[0].contains(&sent.user));
    assert_eq!(lines[1], "AI reply\nnot json");
    // The retry: the earlier reply and the validation error follow.
    assert!(lines[2].contains("[assistant]\nnot json"), "{}", lines[2]);
    assert!(lines[2].contains("Your reply is not valid"), "{}", lines[2]);
    assert_eq!(lines[3], format!("AI reply\n{VALID}"));
}
