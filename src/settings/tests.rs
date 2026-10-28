use super::*;

#[test]
fn a_dotenv_file_is_read_without_comments_quotes_or_blank_lines() {
    let dir = std::env::temp_dir().join(format!("shipscreen-dotenv-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(".env");
    std::fs::write(
        &file,
        "# keys\n\nFAL_KEY=\"abc\"\n  ASC_ISSUER = 'x-y' \nNOT A PAIR\nEMPTY=\n",
    )
    .unwrap();
    let env = parse_dotenv(&file);
    assert_eq!(env.get("FAL_KEY").map(String::as_str), Some("abc"));
    assert_eq!(env.get("ASC_ISSUER").map(String::as_str), Some("x-y"));
    assert_eq!(env.get("EMPTY").map(String::as_str), Some(""));
    assert_eq!(env.len(), 3);
    assert!(parse_dotenv(&dir.join("missing")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
