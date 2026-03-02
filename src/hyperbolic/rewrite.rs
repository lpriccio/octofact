//! Knuth-Bendix confluent rewrite engine for {4,q} hyperbolic tilings.
//!
//! Alphabet: a (move forward & flip), B (turn left), b (turn right).
//! Confluent rewrite rules reduce any word to a unique canonical form.
//! Rules are loaded from `extern/rewrite-pairs-4-{q}.txt`.


/// Byte encoding for the turtle alphabet.
pub const A: u8 = 0; // move forward & flip
pub const B: u8 = 1; // turn left
pub const B_INV: u8 = 2; // turn right (b = B^{-1})

/// A word in the turtle alphabet.
pub type Word = Vec<u8>;

/// A rewrite rule: replace `lhs` with `rhs` wherever found.
#[derive(Clone, Debug)]
pub struct RewriteRule {
    pub lhs: Vec<u8>,
    pub rhs: Vec<u8>,
}

/// Parse a letter string (a, A, B, b) into our byte encoding.
fn parse_letters(s: &str) -> Vec<u8> {
    s.bytes()
        .map(|c| match c {
            b'a' | b'A' => A,
            b'B' => B,
            b'b' => B_INV,
            _ => panic!("invalid letter: {}", c as char),
        })
        .collect()
}

/// Load confluent rewrite rules for {4,q} from `extern/rewrite-pairs-4-{q}.txt`.
///
/// File format (Python repr): `[('lhs', 'rhs'), ...]`
/// Rules are sorted longest-LHS-first for efficient matching.
/// The rule A→a is omitted (handled by our byte encoding).
pub fn load_rules(q: u32) -> Vec<RewriteRule> {
    let path = format!("extern/rewrite-pairs-4-{q}.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {path}: {e}"));

    // Parse Python repr: [('lhs', 'rhs'), ('lhs', 'rhs'), ...]
    // Extract pairs by finding ('...', '...') patterns.
    let mut rules: Vec<RewriteRule> = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '(' {
            // Expect 'lhs'
            assert_eq!(chars.next(), Some('\''), "expected quote in {path}");
            let lhs: String = chars.by_ref().take_while(|&c| c != '\'').collect();
            // Skip ", '"
            chars.by_ref().take_while(|&c| c != '\'').for_each(drop);
            let rhs: String = chars.by_ref().take_while(|&c| c != '\'').collect();
            // Skip to closing ')'
            chars.by_ref().take_while(|&c| c != ')').for_each(drop);

            // Skip A→a rule (handled at parse time)
            if lhs == "A" {
                continue;
            }

            rules.push(RewriteRule {
                lhs: parse_letters(&lhs),
                rhs: parse_letters(&rhs),
            });
        }
    }

    // Sort longest-LHS-first for greedy matching.
    rules.sort_by(|a, b| b.lhs.len().cmp(&a.lhs.len()));
    rules
}

/// Reduce a word by repeatedly applying rewrite rules until no rule matches.
///
/// Strategy: scan left-to-right for the first matching LHS. Replace it with
/// the RHS, then back up the scan position to catch cascading rewrites.
/// Repeat until a full scan finds no match.
pub fn reduce(word: &mut Word, rules: &[RewriteRule]) {
    let max_lhs = rules.iter().map(|r| r.lhs.len()).max().unwrap_or(0);

    let mut pos = 0;
    while pos < word.len() {
        let mut matched = false;
        // Try each rule at this position (longest LHS first due to ordering).
        for rule in rules {
            let lhs_len = rule.lhs.len();
            if pos + lhs_len > word.len() {
                continue;
            }
            if word[pos..pos + lhs_len] == rule.lhs[..] {
                // Replace lhs with rhs in-place.
                word.splice(pos..pos + lhs_len, rule.rhs.iter().copied());
                // Back up to catch cascading rewrites.
                pos = pos.saturating_sub(max_lhs - 1);
                matched = true;
                break;
            }
        }
        if !matched {
            pos += 1;
        }
    }
}

/// Reduce a word, returning the result (non-mutating convenience wrapper).
#[allow(dead_code)]
pub fn reduced(word: &[u8], rules: &[RewriteRule]) -> Word {
    let mut w = word.to_vec();
    reduce(&mut w, rules);
    w
}

/// Shortlex comparison for words.
///
/// Ordering: shorter words first. For equal length, lexicographic with b < B < a.
/// This means byte values map as: a(0)→2, B(1)→1, b(2)→0 for comparison.
pub fn shortlex_cmp(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    // Length dominates.
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {}
        ord => return ord,
    }

    // Same length: lexicographic with b(2) < B(1) < a(0).
    // Map: a(0)→2, B(1)→1, b(2)→0.
    fn sort_key(letter: u8) -> u8 {
        match letter {
            0 => 2, // a is largest
            1 => 1, // B is middle
            2 => 0, // b is smallest
            _ => unreachable!(),
        }
    }

    for (&x, &y) in a.iter().zip(b.iter()) {
        match sort_key(x).cmp(&sort_key(y)) {
            Ordering::Equal => continue,
            ord => return ord,
        }
    }
    Ordering::Equal
}

/// Convert a word to its human-readable string form.
pub fn word_to_string(word: &[u8]) -> String {
    if word.is_empty() {
        return "e".to_string();
    }
    word.iter()
        .map(|&c| match c {
            A => 'a',
            B => 'B',
            B_INV => 'b',
            _ => '?',
        })
        .collect()
}

/// Parse a string into a word. 'e' or empty string → empty word.
#[allow(dead_code)]
pub fn string_to_word(s: &str) -> Word {
    if s.is_empty() || s == "e" {
        return vec![];
    }
    s.bytes()
        .map(|c| match c {
            b'a' | b'A' => A,
            b'B' => B,
            b'b' => B_INV,
            _ => panic!("invalid letter in word: {}", c as char),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    fn r() -> Vec<RewriteRule> {
        load_rules(5)
    }

    // --- Individual rule tests ---

    #[test]
    fn test_rule_bb_cancel() {
        // bB → empty
        assert_eq!(reduced(&string_to_word("bB"), &r()), vec![]);
    }

    #[test]
    fn test_rule_b_b_cancel() {
        // Bb → empty
        assert_eq!(reduced(&string_to_word("Bb"), &r()), vec![]);
    }

    #[test]
    fn test_rule_aa_cancel() {
        // aa → empty
        assert_eq!(reduced(&string_to_word("aa"), &r()), vec![]);
    }

    #[test]
    fn test_rule_uppercase_a() {
        // A → a
        assert_eq!(reduced(&string_to_word("A"), &r()), string_to_word("a"));
    }

    #[test]
    fn test_rule_bbb_to_b_upper() {
        // bbb → B
        assert_eq!(reduced(&string_to_word("bbb"), &r()), string_to_word("B"));
    }

    #[test]
    fn test_rule_bb_upper_to_bb_lower() {
        // BB → bb
        assert_eq!(reduced(&string_to_word("BB"), &r()), string_to_word("bb"));
    }

    #[test]
    fn test_rule_ababa() {
        // ababa → BaBaB
        assert_eq!(
            reduced(&string_to_word("ababa"), &r()),
            string_to_word("BaBaB")
        );
    }

    #[test]
    fn test_rule_ababba() {
        // aBaBa → babab
        assert_eq!(
            reduced(&string_to_word("aBaBa"), &r()),
            string_to_word("babab")
        );
    }

    #[test]
    fn test_rule_9() {
        assert_eq!(
            reduced(&string_to_word("ababbabab"), &r()),
            string_to_word("BaBabbaBa")
        );
    }

    #[test]
    fn test_rule_10() {
        assert_eq!(
            reduced(&string_to_word("aBabbaBaB"), &r()),
            string_to_word("bababbaba")
        );
    }

    #[test]
    fn test_rule_11() {
        assert_eq!(
            reduced(&string_to_word("aBabbaBabb"), &r()),
            string_to_word("bababbabaB")
        );
    }

    // --- Group relations ---

    #[test]
    fn test_b4_is_identity() {
        // B^4 = e: BBBB → BB→bb, then bbBB → bb·bb... let's just check
        // BBBB: BB→bb gives bbBB, BB→bb gives bbbb, bbb→B gives Bb, Bb→e. ✓
        assert_eq!(reduced(&string_to_word("BBBB"), &r()), vec![]);
    }

    #[test]
    fn test_b4_lower_is_identity() {
        // b^4 = e: bbbb → bbb·b → B·b → Bb → e
        assert_eq!(reduced(&string_to_word("bbbb"), &r()), vec![]);
    }

    #[test]
    fn test_vertex_relation_ab5() {
        // (aB)^5 = e
        assert_eq!(
            reduced(&string_to_word("aBaBaBaBaB"), &r()),
            vec![]
        );
    }

    #[test]
    fn test_vertex_relation_ab5_lowercase() {
        // (ab)^5 should also reduce (since b = B^{-1}, this is a different relation)
        // Actually (ab)^5: this is (a·b)^5 which in the group is a·B^{-1} repeated.
        // Let's just check it reduces to something.
        let result = reduced(&string_to_word("ababababab"), &r());
        // (ab)^5 is NOT necessarily identity. Let's just verify it terminates.
        // Reduce again to check idempotence:
        assert_eq!(reduced(&result, &r()), result);
    }

    // --- Cascading reductions ---

    #[test]
    fn test_cascading_inner_cancel() {
        // abBa: inner bB cancels → aa → empty
        assert_eq!(reduced(&string_to_word("abBa"), &r()), vec![]);
    }

    #[test]
    #[allow(non_snake_case)]
    fn test_cascading_aBba() {
        // aBba: inner Bb cancels → aa → empty
        assert_eq!(reduced(&string_to_word("aBba"), &r()), vec![]);
    }

    #[test]
    fn test_cascading_deep() {
        // aabBaa: bB→e → aaaa → aa·aa → each aa→e
        assert_eq!(reduced(&string_to_word("aabBaa"), &r()), vec![]);
    }

    // --- Idempotence ---

    #[test]
    fn test_idempotence() {
        let rules = r();
        let words = ["a", "B", "b", "aB", "Ba", "ab", "ba", "aBa", "BaB", "bab"];
        for s in &words {
            let once = reduced(&string_to_word(s), &rules);
            let twice = reduced(&once, &rules);
            assert_eq!(
                once, twice,
                "reduction of '{}' is not idempotent: {:?} vs {:?}",
                s,
                word_to_string(&once),
                word_to_string(&twice),
            );
        }
    }

    #[test]
    fn test_empty_word() {
        assert_eq!(reduced(&[], &r()), vec![]);
    }

    #[test]
    fn test_single_letters_stable() {
        let rules = r();
        assert_eq!(reduced(&[A], &rules), vec![A]);
        assert_eq!(reduced(&[B], &rules), vec![B]);
        assert_eq!(reduced(&[B_INV], &rules), vec![B_INV]);
    }

    // --- Shortlex ordering ---

    #[test]
    fn test_shortlex_empty_smallest() {
        assert_eq!(shortlex_cmp(&[], &[A]), Ordering::Less);
        assert_eq!(shortlex_cmp(&[], &[B]), Ordering::Less);
        assert_eq!(shortlex_cmp(&[], &[B_INV]), Ordering::Less);
        assert_eq!(shortlex_cmp(&[], &[]), Ordering::Equal);
    }

    #[test]
    fn test_shortlex_length_dominates() {
        // Shorter always wins, even if letters are "bigger"
        assert_eq!(shortlex_cmp(&[A], &[B_INV, B_INV]), Ordering::Less);
    }

    #[test]
    fn test_shortlex_same_length_lex() {
        // b < B < a
        assert_eq!(shortlex_cmp(&[B_INV], &[B]), Ordering::Less); // b < B
        assert_eq!(shortlex_cmp(&[B], &[A]), Ordering::Less); // B < a
        assert_eq!(shortlex_cmp(&[B_INV], &[A]), Ordering::Less); // b < a
    }

    #[test]
    fn test_shortlex_multi_char() {
        // "ba" < "Ba" < "aB" < "aa" (all length 2)
        let ba = [B_INV, A];
        let b_a = [B, A]; // "Ba"
        let ab = [A, B];
        let aa = [A, A];
        assert_eq!(shortlex_cmp(&ba, &b_a), Ordering::Less);
        assert_eq!(shortlex_cmp(&b_a, &ab), Ordering::Less);
        assert_eq!(shortlex_cmp(&ab, &aa), Ordering::Less);
    }

    // --- String conversion ---

    #[test]
    fn test_word_to_string() {
        assert_eq!(word_to_string(&[]), "e");
        assert_eq!(word_to_string(&[A]), "a");
        assert_eq!(word_to_string(&[B]), "B");
        assert_eq!(word_to_string(&[B_INV]), "b");
        assert_eq!(word_to_string(&[A, B, A, B_INV]), "aBab");
    }

    #[test]
    fn test_string_to_word() {
        assert_eq!(string_to_word("e"), vec![]);
        assert_eq!(string_to_word(""), vec![]);
        assert_eq!(string_to_word("aBab"), vec![A, B, A, B_INV]);
    }

    #[test]
    fn test_roundtrip_string() {
        let words = ["e", "a", "B", "b", "aBa", "bab", "BaBaB"];
        for s in &words {
            let w = string_to_word(s);
            let s2 = word_to_string(&w);
            assert_eq!(*s, s2, "roundtrip failed for '{}'", s);
        }
    }
}
