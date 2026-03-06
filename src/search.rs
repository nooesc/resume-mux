use crate::adapters::Session;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy)]
struct SearchMatch {
    index: usize,
    score: f64,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug)]
pub struct SearchResult<'a> {
    pub session: &'a Session,
    pub score: f64,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn search<'a>(query: &str, sessions: &'a [Session]) -> Vec<SearchResult<'a>> {
    search_matches(query, sessions)
        .into_iter()
        .filter_map(|result| {
            sessions.get(result.index).map(|session| SearchResult {
                session,
                score: result.score,
            })
        })
        .collect()
}

pub fn search_indices(query: &str, sessions: &[Session]) -> Vec<usize> {
    search_matches(query, sessions)
        .into_iter()
        .map(|result| result.index)
        .collect()
}

fn search_matches(query: &str, sessions: &[Session]) -> Vec<SearchMatch> {
    let query_trimmed = query.trim();

    if query_trimmed.is_empty() {
        let mut results: Vec<SearchMatch> = sessions
            .iter()
            .enumerate()
            .map(|(index, _)| SearchMatch { index, score: 1.0 })
            .collect();
        results.sort_by(|a, b| {
            sessions[b.index]
                .timestamp
                .cmp(&sessions[a.index].timestamp)
        });
        return results;
    }

    let query_lower = query_trimmed.to_lowercase();
    let tokens: Vec<&str> = query_lower.split_whitespace().collect();

    let now = SystemTime::now();

    let mut results: Vec<SearchMatch> = sessions
        .iter()
        .enumerate()
        .filter_map(|session| {
            let (index, session) = session;
            let title_score = score_field(&query_lower, &tokens, &session.title_lower, true);
            let dir_score = score_field(&query_lower, &tokens, &session.dir_lower, true);
            let content_score = score_field(&query_lower, &tokens, &session.content_lower, false);

            let weighted = title_score * 3.0 + dir_score * 2.0 + content_score * 1.0;

            if weighted == 0.0 {
                return None;
            }

            // Recency tiebreaker: decay over time
            let recency_bonus = recency_decay(now, session.timestamp);
            let score = weighted + recency_bonus;

            Some(SearchMatch { index, score })
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results
}

/// Score a single field against the query and its tokens.
/// When `use_levenshtein` is false, skips typo tolerance (expensive for large fields like content).
fn score_field(query: &str, tokens: &[&str], field: &str, use_levenshtein: bool) -> f64 {
    let mut score = 0.0_f64;

    // 1. Exact full query substring match
    if field.contains(query) {
        score = score.max(100.0);
    }

    // 2. Per-token exact substring match + word boundary bonus
    for token in tokens {
        if field.contains(token) {
            let mut token_score = 50.0;
            if is_at_word_boundary(field, token) {
                token_score += 20.0;
            }
            score = score.max(token_score);
        }
    }

    // 3. Consecutive character match (fzf-style fuzzy)
    let fuzzy = fuzzy_score(query, field);
    score = score.max(fuzzy);

    // 4. Levenshtein typo tolerance per word in the field
    if use_levenshtein {
        let field_words: Vec<&str> = field.split_whitespace().collect();
        for token in tokens {
            for word in &field_words {
                if levenshtein(token, word) <= 1 && token != word {
                    score = score.max(10.0);
                }
            }
        }
    }

    score
}

/// Check if `token` appears at a word boundary in `field`.
fn is_at_word_boundary(field: &str, token: &str) -> bool {
    if let Some(pos) = field.find(token) {
        if pos == 0 {
            return true;
        }
        let prev_char = field[..pos].chars().last().unwrap();
        !prev_char.is_alphanumeric()
    } else {
        false
    }
}

/// Fuzzy (fzf-style) consecutive character matching.
/// Returns ~30 points scaled by match density, plus a contiguous bonus.
fn fuzzy_score(query: &str, field: &str) -> f64 {
    let query_chars: Vec<char> = query.chars().collect();
    let field_chars: Vec<char> = field.chars().collect();

    if query_chars.is_empty() {
        return 0.0;
    }

    // Try to find all query chars in order within the field
    let mut qi = 0;
    let mut match_positions: Vec<usize> = Vec::new();

    for (fi, &fc) in field_chars.iter().enumerate() {
        if qi < query_chars.len() && fc == query_chars[qi] {
            match_positions.push(fi);
            qi += 1;
        }
    }

    // All characters must match
    if qi < query_chars.len() {
        return 0.0;
    }

    let matched = match_positions.len() as f64;
    let span = (match_positions.last().unwrap() - match_positions.first().unwrap() + 1) as f64;

    // Density: how tightly packed the matches are
    let density = matched / span;

    // Contiguous bonus: count consecutive pairs
    let mut contiguous = 0usize;
    for i in 1..match_positions.len() {
        if match_positions[i] == match_positions[i - 1] + 1 {
            contiguous += 1;
        }
    }
    let contiguous_ratio = if match_positions.len() > 1 {
        contiguous as f64 / (match_positions.len() - 1) as f64
    } else {
        1.0
    };

    // Base ~30 points scaled by density and contiguous bonus
    30.0 * density + 15.0 * contiguous_ratio
}

/// Levenshtein edit distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    dp[m][n]
}

/// Recency decay: newer sessions get a small bonus (0.0 to 0.5).
fn recency_decay(now: SystemTime, timestamp: SystemTime) -> f64 {
    let elapsed = now.duration_since(timestamp).unwrap_or_default().as_secs() as f64;
    // Decay over ~30 days (2_592_000 seconds)
    let decay = (-elapsed / 2_592_000.0).exp();
    0.5 * decay
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{Agent, Session};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn make_session(title: &str, dir: &str, content: &str) -> Session {
        Session::new(
            title.to_string(),
            Agent::Claude,
            title.to_string(),
            PathBuf::from(dir),
            SystemTime::now(),
            PathBuf::from("/tmp/session.jsonl"),
            content.to_string(),
            1,
        )
    }

    #[test]
    fn test_exact_substring_ranks_highest() {
        let sessions = vec![
            make_session(
                "fix authentication bug",
                "/home/user/project",
                "unrelated content",
            ),
            make_session(
                "unrelated title",
                "/home/user/project",
                "fix authentication bug in content",
            ),
        ];

        let results = search("fix authentication", &sessions);

        assert!(!results.is_empty(), "should have results");
        // Title match (3x weight) should rank higher than content match (1x weight)
        assert_eq!(
            results[0].session.title, "fix authentication bug",
            "title match should rank first"
        );
        assert!(
            results[0].score > results[1].score,
            "title match score should be higher than content match score"
        );
    }

    #[test]
    fn test_empty_query_returns_all() {
        let mut sessions = vec![
            make_session("session a", "/tmp/a", "content a"),
            make_session("session b", "/tmp/b", "content b"),
            make_session("session c", "/tmp/c", "content c"),
        ];

        // Make session c the newest, session a the oldest
        sessions[0].timestamp = SystemTime::now() - Duration::from_secs(200);
        sessions[1].timestamp = SystemTime::now() - Duration::from_secs(100);
        sessions[2].timestamp = SystemTime::now();

        let results = search("", &sessions);

        assert_eq!(results.len(), 3, "all sessions should be returned");
        // Should be ordered by recency (newest first)
        assert_eq!(results[0].session.title, "session c");
        assert_eq!(results[1].session.title, "session b");
        assert_eq!(results[2].session.title, "session a");
    }

    #[test]
    fn test_fuzzy_character_matching() {
        let sessions = vec![
            make_session("fix authentication", "/home/user/project", "some content"),
            make_session("unrelated session", "/tmp/other", "nothing relevant"),
        ];

        let results = search("fxath", &sessions);

        assert!(!results.is_empty(), "fuzzy match should find results");
        assert_eq!(
            results[0].session.title, "fix authentication",
            "fuzzy match should find 'fix authentication'"
        );
    }

    #[test]
    fn test_directory_matching() {
        let sessions = vec![
            make_session("some session", "/home/user/backend", "unrelated content"),
            make_session("other session", "/home/user/frontend", "unrelated content"),
        ];

        let results = search("backend", &sessions);

        assert!(!results.is_empty(), "directory match should find results");
        assert_eq!(
            results[0].session.title, "some session",
            "should match session with /home/user/backend directory"
        );
    }
}
