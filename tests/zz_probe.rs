use slack_cli::auth::extract::chromium::extract_tokens_from_leveldb;
use std::path::PathBuf;
#[test]
fn list_desktop_workspaces() {
    let home = std::env::var("HOME").unwrap();
    let p = PathBuf::from(&home).join("Library/Application Support/Slack/Local Storage/leveldb");
    let mut t = extract_tokens_from_leveldb(&p).unwrap();
    t.sort_by(|a, b| a.domain.cmp(&b.domain));
    eprintln!("{} workspaces signed into the Slack desktop app:", t.len());
    for x in &t {
        eprintln!(
            "  {:<24} {:<14} {}",
            x.domain.as_deref().unwrap_or("?"),
            x.team_id.as_deref().unwrap_or("?"),
            x.name.as_deref().unwrap_or("?")
        );
    }
}
